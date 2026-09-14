use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use rquickjs::{CaughtError, Context, Function, Runtime, Value};
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);
const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const STACK_LIMIT_BYTES: usize = 512 * 1024;
const MAX_CODE_BYTES: usize = 64 * 1024;
const MAX_LOG_BYTES: usize = 64 * 1024;
const MAX_RESULT_BYTES: usize = 64 * 1024;
const MAX_ERROR_BYTES: usize = 4 * 1024;

const CONSOLE_SETUP: &str = r#"
(() => {
    const lines = [];
    let size = 0;
    let truncated = false;
    const max = 65536;
    const render = (value) => {
        if (typeof value === "string") return value;
        try {
            const json = JSON.stringify(value);
            return json === undefined ? String(value) : json;
        } catch (_) {
            return String(value);
        }
    };
    const append = (level, values) => {
        if (size >= max) {
            truncated = true;
            return;
        }
        const line = `[${level}] ${values.map(render).join(" ")}`;
        const separator = lines.length === 0 ? 0 : 1;
        const remaining = max - size - separator;
        if (remaining <= 0) {
            truncated = true;
            return;
        }
        lines.push(line.slice(0, remaining));
        size += separator + Math.min(line.length, remaining);
        if (line.length > remaining) truncated = true;
    };
    globalThis.console = Object.freeze({
        log: (...values) => append("LOG", values),
        info: (...values) => append("INFO", values),
        warn: (...values) => append("WARN", values),
        error: (...values) => append("ERROR", values),
    });
    return () => JSON.stringify({ logs: lines.join("\n"), truncated });
})()
"#;

pub struct JavaScriptTool;

impl JavaScriptTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JavaScriptTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for JavaScriptTool {
    fn name(&self) -> &str {
        "eval_javascript"
    }

    fn description(&self) -> &str {
        "Execute JavaScript with a sandboxed QuickJS ES2020 engine for calculations, string \
         processing, and JSON transformations. The value of the last expression is returned. \
         console.log/info/warn/error output is captured. DOM, Node.js, network, file, and host \
         APIs are unavailable."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "JavaScript source code to execute",
                    "maxLength": MAX_CODE_BYTES
                }
            },
            "required": ["code"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: JsonValue, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let Some(code) = args.get("code").and_then(JsonValue::as_str) else {
            return Ok(failure("Missing required string parameter: code"));
        };
        if code.len() > MAX_CODE_BYTES {
            return Ok(failure(format!(
                "JavaScript source exceeds the {MAX_CODE_BYTES} byte limit"
            )));
        }

        let code = code.to_owned();
        let execution =
            tokio::task::spawn_blocking(move || evaluate_javascript(&code, EXECUTION_TIMEOUT))
                .await;

        Ok(match execution {
            Ok(Ok(output)) => ToolResult {
                success: true,
                output,
                error: None,
                presentation: None,
            },
            Ok(Err(error)) => failure(error),
            Err(error) => failure(format!("JavaScript worker failed: {error}")),
        })
    }
}

fn evaluate_javascript(code: &str, timeout: Duration) -> Result<String, String> {
    let runtime =
        Runtime::new().map_err(|error| format!("Unable to create QuickJS runtime: {error}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT_BYTES);
    runtime.set_max_stack_size(STACK_LIMIT_BYTES);

    let deadline = Instant::now() + timeout;
    let timed_out = Arc::new(AtomicBool::new(false));
    let interrupt_timed_out = Arc::clone(&timed_out);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let expired = Instant::now() >= deadline;
        if expired {
            interrupt_timed_out.store(true, Ordering::Relaxed);
        }
        expired
    })));

    let context = Context::full(&runtime)
        .map_err(|error| format!("Unable to create QuickJS context: {error}"))?;
    context.with(|ctx| {
        let read_logs: Function = ctx
            .eval(CONSOLE_SETUP)
            .map_err(|error| format_js_error(&ctx, error))?;

        let result: Value = match ctx.eval(code) {
            Ok(result) => result,
            Err(error) => {
                if timed_out.load(Ordering::Relaxed) {
                    return Err(format!(
                        "JavaScript execution exceeded {}ms",
                        timeout.as_millis()
                    ));
                }
                let error = format_js_error(&ctx, error);
                let logs = read_console_logs(&read_logs).unwrap_or_default();
                return Err(error_with_logs(error, &logs));
            }
        };

        let (result, result_truncated) = value_to_json(&ctx, result)?;
        let logs = read_console_logs(&read_logs)?;
        let mut payload = serde_json::Map::new();
        payload.insert("result".to_string(), result);
        if !logs.text.is_empty() {
            payload.insert("logs".to_string(), JsonValue::String(logs.text));
        }
        if logs.truncated {
            payload.insert("logs_truncated".to_string(), JsonValue::Bool(true));
        }
        if result_truncated {
            payload.insert("result_truncated".to_string(), JsonValue::Bool(true));
        }
        Ok(JsonValue::Object(payload).to_string())
    })
}

#[derive(Default, serde::Deserialize)]
struct ConsoleLogs {
    logs: String,
    truncated: bool,
}

#[derive(Default)]
struct BoundedLogs {
    text: String,
    truncated: bool,
}

fn read_console_logs(read_logs: &Function<'_>) -> Result<BoundedLogs, String> {
    let raw: String = read_logs
        .call(())
        .map_err(|error| format!("Unable to read JavaScript console output: {error}"))?;
    let logs: ConsoleLogs = serde_json::from_str(&raw)
        .map_err(|error| format!("Unable to parse JavaScript console output: {error}"))?;
    let (text, clipped) = truncate_utf8(&logs.logs, MAX_LOG_BYTES);
    Ok(BoundedLogs {
        text,
        truncated: logs.truncated || clipped,
    })
}

fn value_to_json<'js>(
    ctx: &rquickjs::Ctx<'js>,
    value: Value<'js>,
) -> Result<(JsonValue, bool), String> {
    if value.is_undefined() || value.is_null() {
        return Ok((JsonValue::Null, false));
    }

    if let Some(value) = value.as_string() {
        let text = value
            .to_string()
            .map_err(|error| format!("Unable to read JavaScript result: {error}"))?;
        let (text, truncated) = truncate_utf8(&text, MAX_RESULT_BYTES);
        return Ok((JsonValue::String(text), truncated));
    }

    match ctx.json_stringify(value.clone()) {
        Ok(Some(serialized)) => {
            let serialized = serialized
                .to_string()
                .map_err(|error| format!("Unable to read JavaScript result: {error}"))?;
            if serialized.len() > MAX_RESULT_BYTES {
                let (text, _) = truncate_utf8(&serialized, MAX_RESULT_BYTES);
                return Ok((JsonValue::String(text), true));
            }
            let value = serde_json::from_str(&serialized)
                .map_err(|error| format!("Unable to parse JavaScript result: {error}"))?;
            Ok((value, false))
        }
        Ok(None) | Err(_) => {
            let stringify: Function = ctx
                .eval("value => String(value)")
                .map_err(|error| format_js_error(ctx, error))?;
            let text: String = stringify
                .call((value,))
                .map_err(|error| format_js_error(ctx, error))?;
            let (text, truncated) = truncate_utf8(&text, MAX_RESULT_BYTES);
            Ok((JsonValue::String(text), truncated))
        }
    }
}

fn format_js_error(ctx: &rquickjs::Ctx<'_>, error: rquickjs::Error) -> String {
    truncate_utf8(
        &CaughtError::from_error(ctx, error).to_string(),
        MAX_ERROR_BYTES,
    )
    .0
}

fn error_with_logs(error: String, logs: &BoundedLogs) -> String {
    if logs.text.is_empty() {
        return error;
    }
    truncate_utf8(
        &format!("{error}\nConsole output:\n{}", logs.text),
        MAX_ERROR_BYTES,
    )
    .0
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
}

fn failure(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(truncate_utf8(&error.into(), MAX_ERROR_BYTES).0),
        presentation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_values_and_captures_console_output() {
        let output = evaluate_javascript(
            "console.log('answer'); const value = 6 * 7; ({ value, items: [1, 4, 9] })",
            Duration::from_secs(1),
        )
        .unwrap();
        let output: JsonValue = serde_json::from_str(&output).unwrap();

        assert_eq!(output["result"]["value"], 42);
        assert_eq!(output["result"]["items"], json!([1, 4, 9]));
        assert_eq!(output["logs"], "[LOG] answer");
    }

    #[test]
    fn does_not_expose_host_apis() {
        let output = evaluate_javascript(
            "({ fetch: typeof fetch, document: typeof document, process: typeof process, require: typeof require })",
            Duration::from_secs(1),
        )
        .unwrap();
        let output: JsonValue = serde_json::from_str(&output).unwrap();

        for name in ["fetch", "document", "process", "require"] {
            assert_eq!(output["result"][name], "undefined");
        }
    }

    #[test]
    fn interrupts_infinite_loops() {
        let started = Instant::now();
        let error = evaluate_javascript("while (true) {}", Duration::from_millis(50)).unwrap_err();

        assert!(error.contains("exceeded 50ms"), "unexpected error: {error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn truncates_large_results_on_utf8_boundaries() {
        let output = evaluate_javascript("'界'.repeat(30000)", Duration::from_secs(1)).unwrap();
        let output: JsonValue = serde_json::from_str(&output).unwrap();

        assert_eq!(output["result_truncated"], true);
        let result = output["result"].as_str().unwrap();
        assert!(result.len() <= MAX_RESULT_BYTES);
        assert!(result.starts_with('界'));
        assert!(result.ends_with('界'));
    }
}
