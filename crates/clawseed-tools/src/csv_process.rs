//! Read, query, inspect, and write CSV files in the workspace.

use crate::util_helpers::{resolve_existing_workspace_path, resolve_workspace_write_path};
use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;

const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
const MAX_ROWS: usize = 200_000;
const MAX_RETURN_ROWS: usize = 1_000;
const DEFAULT_RETURN_ROWS: usize = 100;

pub struct CsvProcessTool;

impl CsvProcessTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CsvProcessTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CsvProcessTool {
    fn name(&self) -> &str {
        "csv_process"
    }

    fn description(&self) -> &str {
        "Inspect, read, filter, sort, and write CSV files in the workspace. Supports quoted fields, escaped quotes, multiline records, configurable delimiters, pagination, and simple typed comparisons."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": { "type": "string", "enum": ["inspect", "read", "query", "write"] },
                "path": { "type": "string", "description": "CSV path relative to the workspace" },
                "delimiter": { "type": "string", "description": "One-character delimiter; defaults to comma" },
                "has_header": { "type": "boolean", "description": "Whether the first row contains column names; defaults to true" },
                "offset": { "type": "integer", "minimum": 0, "description": "Rows to skip for read/query" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Maximum rows returned for read/query" },
                "columns": { "type": "array", "items": { "type": "string" }, "description": "Columns to return for read/query; defaults to all" },
                "filters": { "type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "column": { "type": "string" },
                        "op": { "type": "string", "enum": ["eq", "ne", "contains", "starts_with", "ends_with", "gt", "gte", "lt", "lte"] },
                        "value": { "type": "string" }
                    },
                    "required": ["column", "op", "value"],
                    "additionalProperties": false
                }, "description": "All filters must match" },
                "sort_by": { "type": "string", "description": "Column used to sort query results" },
                "sort_order": { "type": "string", "enum": ["asc", "desc"], "default": "asc" },
                "sample_size": { "type": "integer", "minimum": 0, "maximum": 20, "description": "Rows returned by inspect; defaults to 5" },
                "headers": { "type": "array", "items": { "type": "string" }, "description": "Column names for write" },
                "rows": { "type": "array", "description": "Rows for write; each item may be an array or an object keyed by headers" }
            },
            "required": ["operation", "path"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let operation = match args.get("operation").and_then(Value::as_str) {
            Some(value) if matches!(value, "inspect" | "read" | "query" | "write") => value,
            Some(value) => return Ok(failure(format!("Unknown operation '{value}'"))),
            None => return Ok(failure("Missing required parameter: operation")),
        };
        let path = match args.get("path").and_then(Value::as_str) {
            Some(value) => value,
            None => return Ok(failure("Missing required parameter: path")),
        };
        let delimiter = match delimiter(args.get("delimiter")) {
            Ok(value) => value,
            Err(error) => return Ok(failure(error)),
        };
        let has_header = args
            .get("has_header")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        if operation == "write" {
            return self.write(&args, ctx, path, delimiter).await;
        }

        let resolved = match resolve_existing_workspace_path(ctx.workspace_dir(), path) {
            Ok(value) => value,
            Err(error) => return Ok(failure(error)),
        };
        let bytes = match tokio::fs::read(&resolved).await {
            Ok(value) => value,
            Err(error) => return Ok(failure(format!("Failed to read CSV file: {error}"))),
        };
        if bytes.len() > MAX_FILE_BYTES {
            return Ok(failure(format!(
                "CSV file is too large: {} bytes (limit: {MAX_FILE_BYTES})",
                bytes.len()
            )));
        }
        let text = match decode_utf8(&bytes) {
            Ok(value) => value,
            Err(error) => return Ok(failure(error)),
        };
        let rows = match parse_csv(&text, delimiter) {
            Ok(value) => value,
            Err(error) => return Ok(failure(error)),
        };
        if rows.len() > MAX_ROWS {
            return Ok(failure(format!(
                "CSV contains too many rows: {} (limit: {MAX_ROWS})",
                rows.len()
            )));
        }
        let table = match Table::new(rows, has_header) {
            Ok(value) => value,
            Err(error) => return Ok(failure(error)),
        };

        let output = match operation {
            "inspect" => inspect(&table, &args, delimiter),
            "read" => select_rows(&table, &args, false),
            "query" => select_rows(&table, &args, true),
            _ => unreachable!(),
        };
        match output {
            Ok(value) => Ok(success(value)),
            Err(error) => Ok(failure(error)),
        }
    }
}

impl CsvProcessTool {
    async fn write(
        &self,
        args: &Value,
        ctx: &dyn ToolContext,
        path: &str,
        delimiter: char,
    ) -> anyhow::Result<ToolResult> {
        let headers = match args.get("headers").and_then(Value::as_array) {
            Some(values) if !values.is_empty() => match values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "headers must contain only strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(headers) => headers,
                Err(error) => return Ok(failure(error)),
            },
            _ => return Ok(failure("write requires a non-empty headers array")),
        };
        if let Err(error) = validate_headers(&headers) {
            return Ok(failure(error));
        }
        let rows = match args.get("rows").and_then(Value::as_array) {
            Some(rows) => rows,
            None => return Ok(failure("write requires a rows array")),
        };
        if rows.len() > MAX_ROWS {
            return Ok(failure(format!(
                "Too many rows: {} (limit: {MAX_ROWS})",
                rows.len()
            )));
        }
        let mut output = String::new();
        output.push_str(&encode_row(&headers, delimiter));
        for row in rows {
            let fields = if let Some(values) = row.as_array() {
                if values.len() != headers.len() {
                    return Ok(failure(format!(
                        "Row has {} fields but headers have {} columns",
                        values.len(),
                        headers.len()
                    )));
                }
                match values
                    .iter()
                    .map(value_to_string)
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(fields) => fields,
                    Err(error) => return Ok(failure(error.to_string())),
                }
            } else if let Some(values) = row.as_object() {
                if let Some(extra) = values
                    .keys()
                    .find(|key| !headers.iter().any(|header| header == *key))
                {
                    return Ok(failure(format!("Row contains unknown column '{extra}'")));
                }
                match headers
                    .iter()
                    .map(|header| match values.get(header) {
                        Some(value) => value_to_string(value),
                        None => Ok(String::new()),
                    })
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(fields) => fields,
                    Err(error) => return Ok(failure(error.to_string())),
                }
            } else {
                return Ok(failure("Each row must be an array or object"));
            };
            output.push_str(&encode_row(&fields, delimiter));
        }
        if output.len() > MAX_FILE_BYTES {
            return Ok(failure(format!(
                "Output CSV is too large (limit: {MAX_FILE_BYTES} bytes)"
            )));
        }
        let destination = match resolve_workspace_write_path(ctx.workspace_dir(), path) {
            Ok(value) => value,
            Err(error) => return Ok(failure(error)),
        };
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&destination, output.as_bytes()).await?;
        Ok(success(
            json!({ "path": path, "rows_written": rows.len(), "columns": headers.len() }),
        ))
    }
}

struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn new(mut rows: Vec<Vec<String>>, has_header: bool) -> Result<Self, String> {
        if rows.is_empty() {
            return Ok(Self {
                headers: Vec::new(),
                rows,
            });
        }
        let width = rows[0].len();
        if width == 0 {
            return Err("CSV has no columns".into());
        }
        if rows.iter().any(|row| row.len() != width) {
            return Err("CSV rows have inconsistent column counts".into());
        }
        let headers = if has_header {
            let headers = rows.remove(0);
            validate_headers(&headers)?;
            headers
        } else {
            (1..=width).map(|index| format!("column_{index}")).collect()
        };
        Ok(Self { headers, rows })
    }
}

fn inspect(table: &Table, args: &Value, delimiter: char) -> Result<Value, String> {
    let sample_size = args
        .get("sample_size")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .min(20) as usize;
    let sample = table
        .rows
        .iter()
        .take(sample_size)
        .map(|row| row_object(&table.headers, row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({ "columns": table.headers, "row_count": table.rows.len(), "sample": sample, "delimiter": delimiter.to_string(), "encoding": "utf-8" }),
    )
}

fn select_rows(table: &Table, args: &Value, apply_query: bool) -> Result<Value, String> {
    let selected = requested_columns(table, args.get("columns"))?;
    let filters = if apply_query {
        parse_filters(table, args.get("filters"))?
    } else {
        Vec::new()
    };
    let sort_by = if apply_query {
        args.get("sort_by")
            .and_then(Value::as_str)
            .map(|name| column_index(table, name))
            .transpose()?
    } else {
        None
    };
    let sort_order = args
        .get("sort_order")
        .and_then(Value::as_str)
        .unwrap_or("asc");
    if !matches!(sort_order, "asc" | "desc") {
        return Err("sort_order must be 'asc' or 'desc'".into());
    }
    let descending = sort_order == "desc";
    let mut indices: Vec<usize> = (0..table.rows.len())
        .filter(|index| {
            filters
                .iter()
                .all(|filter| filter.matches(&table.rows[*index]))
        })
        .collect();
    let total_matching_rows = indices.len();
    if let Some(column) = sort_by {
        indices.sort_by(|left, right| {
            let ordering = compare_values(&table.rows[*left][column], &table.rows[*right][column]);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_RETURN_ROWS as u64)
        .min(MAX_RETURN_ROWS as u64) as usize;
    let rows = indices
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|index| {
            let values = selected
                .iter()
                .map(|column| table.rows[index][*column].clone())
                .collect::<Vec<_>>();
            let object = selected
                .iter()
                .zip(values)
                .map(|(column, value)| (table.headers[*column].clone(), Value::String(value)))
                .collect::<Map<_, _>>();
            Value::Object(object)
        })
        .collect::<Vec<_>>();
    Ok(
        json!({ "columns": selected.iter().map(|index| &table.headers[*index]).collect::<Vec<_>>(), "rows": rows, "total_matching_rows": total_matching_rows, "offset": offset, "limit": limit }),
    )
}

fn requested_columns(table: &Table, value: Option<&Value>) -> Result<Vec<usize>, String> {
    match value {
        None => Ok((0..table.headers.len()).collect()),
        Some(value) => value
            .as_array()
            .ok_or_else(|| "columns must be an array of strings".to_string())?
            .iter()
            .map(|name| {
                name.as_str()
                    .ok_or_else(|| "columns must contain only strings".to_string())
                    .and_then(|name| column_index(table, name))
            })
            .collect(),
    }
}

#[derive(Debug)]
struct Filter {
    column: usize,
    op: String,
    value: String,
}

impl Filter {
    fn matches(&self, row: &[String]) -> bool {
        let actual = row.get(self.column).map(String::as_str).unwrap_or("");
        match self.op.as_str() {
            "eq" => actual == self.value,
            "ne" => actual != self.value,
            "contains" => actual.contains(&self.value),
            "starts_with" => actual.starts_with(&self.value),
            "ends_with" => actual.ends_with(&self.value),
            "gt" => compare_values(actual, &self.value) == Ordering::Greater,
            "gte" => compare_values(actual, &self.value) != Ordering::Less,
            "lt" => compare_values(actual, &self.value) == Ordering::Less,
            "lte" => compare_values(actual, &self.value) != Ordering::Greater,
            _ => false,
        }
    }
}

fn parse_filters(table: &Table, value: Option<&Value>) -> Result<Vec<Filter>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| "filters must be an array".to_string())?
        .iter()
        .map(|item| {
            let object = item
                .as_object()
                .ok_or_else(|| "Each filter must be an object".to_string())?;
            let name = object
                .get("column")
                .and_then(Value::as_str)
                .ok_or_else(|| "Filter column is required".to_string())?;
            let op = object
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| "Filter op is required".to_string())?;
            if !matches!(
                op,
                "eq" | "ne"
                    | "contains"
                    | "starts_with"
                    | "ends_with"
                    | "gt"
                    | "gte"
                    | "lt"
                    | "lte"
            ) {
                return Err(format!("Unknown filter op '{op}'"));
            }
            let filter_value = object
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(|| "Filter value must be a string".to_string())?;
            Ok(Filter {
                column: column_index(table, name)?,
                op: op.into(),
                value: filter_value.into(),
            })
        })
        .collect()
}

fn column_index(table: &Table, name: &str) -> Result<usize, String> {
    table
        .headers
        .iter()
        .position(|header| header == name)
        .ok_or_else(|| format!("Unknown CSV column '{name}'"))
}

fn row_object(headers: &[String], row: &[String]) -> Result<Value, String> {
    if headers.len() != row.len() {
        return Err("CSV row width does not match headers".into());
    }
    Ok(Value::Object(
        headers
            .iter()
            .cloned()
            .zip(row.iter().cloned().map(Value::String))
            .collect(),
    ))
}

fn compare_values(left: &str, right: &str) -> Ordering {
    match (left.trim().parse::<f64>(), right.trim().parse::<f64>()) {
        (Ok(left), Ok(right)) => left.partial_cmp(&right).unwrap_or(Ordering::Equal),
        _ => left.cmp(right),
    }
}

fn delimiter(value: Option<&Value>) -> Result<char, String> {
    let value = value.and_then(Value::as_str).unwrap_or(",");
    let mut chars = value.chars();
    let Some(delimiter) = chars.next() else {
        return Err("delimiter must be one character".into());
    };
    if chars.next().is_some() || matches!(delimiter, '"' | '\r' | '\n') {
        return Err("delimiter must be one character and cannot be quote or newline".into());
    }
    Ok(delimiter)
}

fn decode_utf8(bytes: &[u8]) -> Result<String, String> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec())
        .map_err(|_| "CSV must be UTF-8 (optionally with a UTF-8 BOM)".into())
}

fn parse_csv(source: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_start = true;
    let mut record_started = false;
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if in_quotes {
            if character == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    field.push('"');
                    index += 1;
                } else {
                    in_quotes = false;
                    after_quote = true;
                }
            } else {
                field.push(character);
            }
        } else if after_quote {
            if character == delimiter {
                row.push(std::mem::take(&mut field));
                field_start = true;
                after_quote = false;
                record_started = true;
            } else if character == '\r' || character == '\n' {
                let has_record = record_started || !row.is_empty() || !field.is_empty();
                if has_record {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                } else {
                    field.clear();
                    row.clear();
                }
                field_start = true;
                after_quote = false;
                record_started = false;
                if character == '\r' && chars.get(index + 1) == Some(&'\n') {
                    index += 1;
                }
            } else {
                return Err(
                    "Invalid CSV: characters after a closing quote must be a delimiter or newline"
                        .into(),
                );
            }
        } else if character == '"' {
            if !field_start {
                return Err("Invalid CSV: quote must start a field".into());
            }
            in_quotes = true;
            field_start = false;
            record_started = true;
        } else if character == delimiter {
            row.push(std::mem::take(&mut field));
            field_start = true;
            record_started = true;
        } else if character == '\r' || character == '\n' {
            let has_record = record_started || !row.is_empty() || !field.is_empty();
            if has_record {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            } else {
                field.clear();
                row.clear();
            }
            field_start = true;
            record_started = false;
            if character == '\r' && chars.get(index + 1) == Some(&'\n') {
                index += 1;
            }
        } else {
            field.push(character);
            field_start = false;
            record_started = true;
        }
        index += 1;
    }
    if in_quotes {
        return Err("Invalid CSV: unterminated quoted field".into());
    }
    if record_started || !row.is_empty() || !field.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn encode_row(fields: &[String], delimiter: char) -> String {
    let mut output = fields
        .iter()
        .map(|field| {
            if field.contains(delimiter)
                || field.contains('"')
                || field.contains('\r')
                || field.contains('\n')
            {
                format!("\"{}\"", field.replace('"', "\"\""))
            } else {
                field.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(&delimiter.to_string());
    output.push('\n');
    output
}

fn validate_headers(headers: &[String]) -> Result<(), String> {
    if headers.iter().any(|header| header.is_empty()) {
        return Err("CSV headers must not be empty".into());
    }
    let mut seen = std::collections::HashSet::new();
    if headers.iter().any(|header| !seen.insert(header)) {
        return Err("CSV headers must be unique".into());
    }
    Ok(())
}

fn value_to_string(value: &Value) -> Result<String, anyhow::Error> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Null => Ok(String::new()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err(anyhow::anyhow!(
            "CSV cell values must be strings, numbers, booleans, or null"
        )),
    }
}

fn success(value: Value) -> ToolResult {
    ToolResult {
        success: true,
        output: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()),
        error: None,
        presentation: None,
    }
}
fn failure(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(error.into()),
        presentation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_multiline_csv() {
        let rows = parse_csv("name,note\nAlice,\"line one\nline two\"\n", ',').unwrap();
        assert_eq!(rows[1][1], "line one\nline two");
    }

    #[test]
    fn writes_and_reparses_escaped_values() {
        let csv = encode_row(&["name".into(), "note".into()], ',')
            + &encode_row(&["A".into(), "x,y\"z".into()], ',');
        assert_eq!(parse_csv(&csv, ',').unwrap()[1][1], "x,y\"z");
    }

    #[test]
    fn rejects_inconsistent_rows() {
        assert!(Table::new(parse_csv("a,b\n1\n", ',').unwrap(), true).is_err());
    }
}
