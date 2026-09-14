use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::time::Duration;

pub const ASK_USER_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_QUESTION_CHARS: usize = 500;
const MAX_TEXT_CHARS: usize = 1000;
const MAX_OPTIONS: usize = 20;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskUserKind {
    Confirm,
    SingleSelect,
    MultiSelect,
    Text,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AskUserOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AskUserInput {
    #[serde(rename = "type")]
    pub kind: AskUserKind,
    pub question: String,
    #[serde(default)]
    pub options: Vec<AskUserOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AskUserRequest {
    pub request_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub tool_call_id: String,
    #[serde(flatten)]
    pub input: AskUserInput,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AskUserStatus {
    Accepted,
    Declined,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AskUserResponse {
    pub status: AskUserStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Value>,
}

#[async_trait]
pub trait AskUserHandler: Send + Sync {
    async fn ask(&self, request: AskUserRequest) -> Result<AskUserResponse, String>;
}

pub struct AskUserTool {
    handler: Arc<dyn AskUserHandler>,
}

impl AskUserTool {
    pub fn new(handler: Arc<dyn AskUserHandler>) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the current user one structured question when their confirmation or input is required. Never request passwords, API keys, access tokens, payment details, or other secrets."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "type": {"type": "string", "enum": ["confirm", "single_select", "multi_select", "text"]},
                "question": {"type": "string", "minLength": 1, "maxLength": MAX_QUESTION_CHARS},
                "options": {
                    "type": "array", "maxItems": MAX_OPTIONS,
                    "items": {
                        "type": "object", "additionalProperties": false,
                        "properties": {
                            "id": {"type": "string", "minLength": 1, "maxLength": 80},
                            "label": {"type": "string", "minLength": 1, "maxLength": 120},
                            "description": {"type": "string", "maxLength": 240}
                        },
                        "required": ["id", "label"]
                    }
                },
                "placeholder": {"type": "string", "maxLength": 120}
            },
            "required": ["type", "question"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let input: AskUserInput = match serde_json::from_value(args) {
            Ok(input) => input,
            Err(error) => return Ok(tool_error("invalid_request", error.to_string())),
        };
        if let Err(error) = validate_input(&input) {
            return Ok(tool_error("invalid_request", error));
        }
        let Some(user) = ctx.user_context() else {
            return Ok(tool_error(
                "interaction_unavailable",
                "No interactive user session is attached",
            ));
        };
        let (Some(session_id), Some(turn_id), Some(tool_call_id)) = (
            user.session_id.as_deref(),
            ctx.turn_id(),
            ctx.tool_call_id(),
        ) else {
            return Ok(tool_error(
                "interaction_unavailable",
                "Interactive request identity is unavailable",
            ));
        };

        let request = AskUserRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            tool_call_id: tool_call_id.to_string(),
            input,
        };
        let response = match tokio::time::timeout(ASK_USER_TIMEOUT, self.handler.ask(request)).await
        {
            Ok(Ok(response)) => response,
            Ok(Err(message)) => return Ok(tool_error("interaction_unavailable", message)),
            Err(_) => AskUserResponse {
                status: AskUserStatus::Expired,
                answer: None,
            },
        };
        Ok(ToolResult {
            success: true,
            output: serde_json::to_string(&response)?,
            error: None,
            presentation: None,
        })
    }
}

fn tool_error(code: &str, message: impl Into<String>) -> ToolResult {
    let message = message.into();
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(json!({"code": code, "message": message}).to_string()),
        presentation: None,
    }
}

pub fn validate_input(input: &AskUserInput) -> Result<(), String> {
    let question = input.question.trim();
    if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
        return Err(format!(
            "question must contain 1-{MAX_QUESTION_CHARS} characters"
        ));
    }
    if contains_sensitive_request(question)
        || input
            .placeholder
            .as_deref()
            .is_some_and(contains_sensitive_request)
        || input.options.iter().any(|option| {
            contains_sensitive_request(&option.label)
                || option
                    .description
                    .as_deref()
                    .is_some_and(contains_sensitive_request)
        })
    {
        return Err(
            "ask_user cannot collect passwords, credentials, access tokens, or payment information"
                .into(),
        );
    }
    match input.kind {
        AskUserKind::Confirm | AskUserKind::Text if !input.options.is_empty() => {
            return Err("options are only allowed for selection questions".into());
        }
        AskUserKind::SingleSelect | AskUserKind::MultiSelect
            if input.options.is_empty() || input.options.len() > MAX_OPTIONS =>
        {
            return Err(format!(
                "selection questions require 1-{MAX_OPTIONS} options"
            ));
        }
        _ => {}
    }
    let mut ids = std::collections::HashSet::new();
    for option in &input.options {
        if option.id.trim().is_empty() || option.label.trim().is_empty() {
            return Err("option id and label cannot be empty".into());
        }
        if option.id.chars().count() > 80
            || option.label.chars().count() > 120
            || option
                .description
                .as_ref()
                .is_some_and(|description| description.chars().count() > 240)
        {
            return Err("option id, label, or description exceeds its length limit".into());
        }
        if !ids.insert(option.id.as_str()) {
            return Err(format!("duplicate option id: {}", option.id));
        }
    }
    if input
        .placeholder
        .as_ref()
        .is_some_and(|placeholder| placeholder.chars().count() > 120)
    {
        return Err("placeholder exceeds 120 characters".into());
    }
    Ok(())
}

pub fn validate_response(input: &AskUserInput, response: &AskUserResponse) -> Result<(), String> {
    match response.status {
        AskUserStatus::Declined | AskUserStatus::Cancelled | AskUserStatus::Expired => {
            if response.answer.is_some() {
                return Err("non-accepted responses cannot include an answer".into());
            }
            Ok(())
        }
        AskUserStatus::Accepted => {
            let answer = response
                .answer
                .as_ref()
                .ok_or("accepted response requires an answer")?;
            match input.kind {
                AskUserKind::Confirm => answer
                    .as_bool()
                    .filter(|value| *value)
                    .map(|_| ())
                    .ok_or_else(|| "accepted confirmation answer must be true".into()),
                AskUserKind::SingleSelect => {
                    let id = answer
                        .as_str()
                        .ok_or("single_select answer must be an option id")?;
                    input
                        .options
                        .iter()
                        .any(|option| option.id == id)
                        .then_some(())
                        .ok_or_else(|| "single_select answer is not a listed option".into())
                }
                AskUserKind::MultiSelect => {
                    let ids = answer
                        .as_array()
                        .ok_or("multi_select answer must be an array")?;
                    let mut unique = std::collections::HashSet::new();
                    for id in ids {
                        let id = id
                            .as_str()
                            .ok_or("multi_select values must be option ids")?;
                        if !unique.insert(id) || !input.options.iter().any(|option| option.id == id)
                        {
                            return Err(
                                "multi_select answer contains an invalid or duplicate option"
                                    .into(),
                            );
                        }
                    }
                    Ok(())
                }
                AskUserKind::Text => {
                    let text = answer.as_str().ok_or("text answer must be a string")?;
                    if text.chars().count() > MAX_TEXT_CHARS {
                        Err(format!("text answer exceeds {MAX_TEXT_CHARS} characters"))
                    } else {
                        Ok(())
                    }
                }
            }
        }
    }
}

fn contains_sensitive_request(value: &str) -> bool {
    let value = value.to_lowercase();
    [
        "password",
        "passcode",
        "api key",
        "api_key",
        "access token",
        "secret key",
        "credit card",
        "card number",
        "cvv",
        "payment",
        "bank account",
        "密码",
        "口令",
        "密钥",
        "访问令牌",
        "银行卡",
        "信用卡",
        "支付信息",
    ]
    .iter()
    .any(|term| value.contains(term))
}

pub struct CliAskUserHandler;

#[async_trait]
impl AskUserHandler for CliAskUserHandler {
    async fn ask(&self, request: AskUserRequest) -> Result<AskUserResponse, String> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("stdin is not an interactive terminal".into());
        }
        Ok(
            tokio::task::spawn_blocking(move || prompt_cli(&request.input))
                .await
                .unwrap_or(AskUserResponse {
                    status: AskUserStatus::Cancelled,
                    answer: None,
                }),
        )
    }
}

fn prompt_cli(input: &AskUserInput) -> AskUserResponse {
    println!("\n{}", input.question);
    for option in &input.options {
        println!("  {}: {}", option.id, option.label);
    }
    let prompt = match input.kind {
        AskUserKind::Confirm => "Confirm? [y/N]: ",
        AskUserKind::SingleSelect => "Choose one option id (blank to decline): ",
        AskUserKind::MultiSelect => "Choose option ids separated by commas (blank accepts none): ",
        AskUserKind::Text => "Answer (blank to decline): ",
    };
    loop {
        print!("{prompt}");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
            return AskUserResponse {
                status: AskUserStatus::Cancelled,
                answer: None,
            };
        }
        let line = line.trim();
        let response = match input.kind {
            AskUserKind::Confirm
                if line.eq_ignore_ascii_case("y") || line.eq_ignore_ascii_case("yes") =>
            {
                AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!(true)),
                }
            }
            AskUserKind::Confirm => AskUserResponse {
                status: AskUserStatus::Declined,
                answer: None,
            },
            AskUserKind::SingleSelect if line.is_empty() => AskUserResponse {
                status: AskUserStatus::Declined,
                answer: None,
            },
            AskUserKind::SingleSelect => AskUserResponse {
                status: AskUserStatus::Accepted,
                answer: Some(json!(line)),
            },
            AskUserKind::MultiSelect => AskUserResponse {
                status: AskUserStatus::Accepted,
                answer: Some(json!(
                    line.split(',')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                )),
            },
            AskUserKind::Text if line.is_empty() => AskUserResponse {
                status: AskUserStatus::Declined,
                answer: None,
            },
            AskUserKind::Text => AskUserResponse {
                status: AskUserStatus::Accepted,
                answer: Some(json!(line)),
            },
        };
        if validate_response(input, &response).is_ok() {
            return response;
        }
        println!("Invalid answer; please try again.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(kind: AskUserKind) -> AskUserInput {
        AskUserInput {
            kind,
            question: "Choose a target".into(),
            options: vec![
                AskUserOption {
                    id: "a".into(),
                    label: "Alpha".into(),
                    description: None,
                },
                AskUserOption {
                    id: "b".into(),
                    label: "Beta".into(),
                    description: None,
                },
            ],
            placeholder: None,
        }
    }

    #[test]
    fn validates_each_response_shape() {
        assert!(
            validate_response(
                &selection(AskUserKind::SingleSelect),
                &AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!("a"))
                }
            )
            .is_ok()
        );
        assert!(
            validate_response(
                &selection(AskUserKind::MultiSelect),
                &AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!(["a", "b"]))
                }
            )
            .is_ok()
        );
        assert!(
            validate_response(
                &AskUserInput {
                    kind: AskUserKind::Confirm,
                    question: "Continue?".into(),
                    options: vec![],
                    placeholder: None
                },
                &AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!(true))
                }
            )
            .is_ok()
        );
        assert!(
            validate_response(
                &AskUserInput {
                    kind: AskUserKind::Text,
                    question: "Name?".into(),
                    options: vec![],
                    placeholder: None
                },
                &AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!("Ada"))
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_invalid_and_sensitive_requests() {
        let mut input = selection(AskUserKind::SingleSelect);
        input.options.push(input.options[0].clone());
        assert!(validate_input(&input).unwrap_err().contains("duplicate"));
        input = AskUserInput {
            kind: AskUserKind::Text,
            question: "Enter your API key".into(),
            options: vec![],
            placeholder: None,
        };
        assert!(
            validate_input(&input)
                .unwrap_err()
                .contains("cannot collect")
        );
    }

    #[test]
    fn rejects_unlisted_and_duplicate_answers() {
        let input = selection(AskUserKind::MultiSelect);
        assert!(
            validate_response(
                &input,
                &AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!(["a", "a"]))
                }
            )
            .is_err()
        );
        assert!(
            validate_response(
                &input,
                &AskUserResponse {
                    status: AskUserStatus::Accepted,
                    answer: Some(json!(["c"]))
                }
            )
            .is_err()
        );
    }
}
