//! Minimal MCP (Model Context Protocol) JSON-RPC handler.
//!
//! Implements the subset of MCP the `@ccusage/mcp` package exposes: the
//! `initialize`, `notifications/initialized`, `ping`, `tools/list`, and
//! `tools/call` methods over JSON-RPC 2.0. Tool calls run the existing
//! in-process report builders, so no subprocess is spawned.

use serde_json::{json, Value};

use crate::{
    cli::{AgentReportKind, CostMode, McpArgs, SharedArgs},
    commands::{build_blocks_json, build_daily_json, build_monthly_json, build_session_json},
};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "ccusage";
const DEFAULT_BLOCK_HOURS: f64 = 5.0;

struct Tool {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

fn claude_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "since": {"type": "string", "pattern": "^[0-9]{8}$", "description": "Start date (YYYYMMDD)."},
            "until": {"type": "string", "pattern": "^[0-9]{8}$", "description": "End date (YYYYMMDD, inclusive)."},
            "mode": {"type": "string", "enum": ["auto", "calculate", "display"], "default": "auto", "description": "Cost calculation mode."},
            "timezone": {"type": "string", "description": "IANA timezone for date grouping."},
            "locale": {"type": "string", "description": "Accepted for compatibility; dates use ISO formatting."}
        }
    })
}

fn codex_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "since": {"type": "string", "description": "Start date (YYYYMMDD or YYYY-MM-DD)."},
            "until": {"type": "string", "description": "End date (inclusive)."},
            "timezone": {"type": "string", "description": "IANA timezone for date grouping."},
            "locale": {"type": "string", "description": "Accepted for compatibility."},
            "offline": {"type": "boolean", "description": "Use embedded/offline pricing."}
        }
    })
}

fn tools() -> Vec<Tool> {
    let claude = claude_input_schema();
    let codex = codex_input_schema();
    vec![
        Tool {
            name: "daily",
            description: "Show usage report grouped by date",
            input_schema: claude.clone(),
        },
        Tool {
            name: "session",
            description: "Show usage report grouped by conversation session",
            input_schema: claude.clone(),
        },
        Tool {
            name: "monthly",
            description: "Show usage report grouped by month",
            input_schema: claude.clone(),
        },
        Tool {
            name: "blocks",
            description: "Show usage report grouped by session billing blocks",
            input_schema: claude,
        },
        Tool {
            name: "codex-daily",
            description: "Show Codex usage grouped by day",
            input_schema: codex.clone(),
        },
        Tool {
            name: "codex-monthly",
            description: "Show Codex usage grouped by month",
            input_schema: codex,
        },
    ]
}

/// Handles a single JSON-RPC request, returning the response to write (if any).
/// Notifications (no `id`) produce no response.
pub(crate) fn handle_request(request: &Value, args: &McpArgs) -> Option<Value> {
    let method = request.get("method").and_then(Value::as_str)?;
    let id = request.get("id").cloned();
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id?,
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }
        })),
        "notifications/initialized" => None,
        "ping" => Some(json!({"jsonrpc": "2.0", "id": id?, "result": {}})),
        "tools/list" => {
            let tools = tools()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                    })
                })
                .collect::<Vec<_>>();
            Some(json!({"jsonrpc": "2.0", "id": id?, "result": {"tools": tools}}))
        }
        "tools/call" => Some(json!({
            "jsonrpc": "2.0",
            "id": id?,
            "result": handle_tool_call(&params, args)
        })),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id?,
            "error": {"code": -32601, "message": format!("Method not found: {method}")}
        })),
    }
}

fn handle_tool_call(params: &Value, args: &McpArgs) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    if !tools().iter().any(|tool| tool.name == name) {
        return error_text(&format!("Tool {name} not found"));
    }

    match name {
        "daily" | "session" | "monthly" | "blocks" => match claude_shared_from_args(&arguments, args)
        {
            Ok(shared) => {
                let result = match name {
                    "daily" => build_daily_json(&shared),
                    "session" => build_session_json(&shared),
                    "monthly" => build_monthly_json(&shared),
                    "blocks" => build_blocks_json(&shared, DEFAULT_BLOCK_HOURS),
                    _ => unreachable!(),
                };
                match result {
                    Ok(value) => ok_text(&value),
                    Err(_) => fallback_empty(name),
                }
            }
            Err(message) => error_text(&message),
        },
        "codex-daily" | "codex-monthly" => match codex_shared_from_args(&arguments, args) {
            Ok(shared) => {
                let kind = if name == "codex-daily" {
                    AgentReportKind::Daily
                } else {
                    AgentReportKind::Monthly
                };
                match crate::adapter::codex::build_report_json(&shared, kind) {
                    Ok(value) => ok_text(&value),
                    Err(message) => error_text(&format!("{message}")),
                }
            }
            Err(message) => error_text(&message),
        },
        _ => error_text(&format!("Tool {name} not found")),
    }
}

fn opt_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn claude_shared_from_args(arguments: &Value, args: &McpArgs) -> Result<SharedArgs, String> {
    let since = opt_string(arguments, "since");
    let until = opt_string(arguments, "until");
    for value in [since.as_deref(), until.as_deref()].into_iter().flatten() {
        if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Date must be in YYYYMMDD format".to_string());
        }
    }
    let mode = match opt_string(arguments, "mode").as_deref() {
        None | Some("auto") => CostMode::Auto,
        Some("calculate") => CostMode::Calculate,
        Some("display") => CostMode::Display,
        Some(_) => return Err("Invalid cost mode".to_string()),
    };
    let mut shared = SharedArgs::default();
    shared.mode = mode;
    shared.offline = args.shared.offline;
    shared.since = since;
    shared.until = until;
    shared.timezone = opt_string(arguments, "timezone");
    Ok(shared)
}

fn codex_shared_from_args(arguments: &Value, args: &McpArgs) -> Result<SharedArgs, String> {
    let mut shared = SharedArgs::default();
    shared.offline = arguments
        .get("offline")
        .and_then(Value::as_bool)
        .unwrap_or(args.shared.offline);
    shared.since = opt_string(arguments, "since").map(|value| value.replace('-', ""));
    shared.until = opt_string(arguments, "until").map(|value| value.replace('-', ""));
    shared.timezone = opt_string(arguments, "timezone");
    Ok(shared)
}

fn ok_text(value: &Value) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    json!({"content": [{"type": "text", "text": text}]})
}

fn error_text(message: &str) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

fn fallback_empty(name: &str) -> Value {
    let value = match name {
        "daily" => json!({"daily": [], "totals": {}}),
        "session" => json!({"sessions": [], "totals": {}}),
        "monthly" => json!({"monthly": [], "totals": {}}),
        "blocks" => json!({"blocks": []}),
        _ => json!({}),
    };
    ok_text(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::McpTransport;

    fn args() -> McpArgs {
        McpArgs {
            shared: SharedArgs::default(),
            transport: McpTransport::Stdio,
            port: 8080,
        }
    }

    fn call(name: &str, arguments: Value) -> Value {
        handle_tool_call(&json!({"name": name, "arguments": arguments}), &args())
    }

    #[test]
    fn lists_all_six_tools() {
        let response = handle_request(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}), &args()).unwrap();
        let names = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["daily", "session", "monthly", "blocks", "codex-daily", "codex-monthly"]
        );
    }

    #[test]
    fn initialize_advertises_server_info() {
        let response = handle_request(&json!({"jsonrpc": "2.0", "id": 7, "method": "initialize"}), &args()).unwrap();
        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["serverInfo"]["name"], "ccusage");
        assert!(response["result"]["protocolVersion"].is_string());
    }

    #[test]
    fn unknown_tool_returns_error() {
        let result = call("does-not-exist", json!({}));
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Tool does-not-exist not found"));
    }

    #[test]
    fn invalid_mode_returns_error() {
        let result = call("daily", json!({"mode": "bogus"}));
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("Invalid"));
    }

    #[test]
    fn invalid_date_returns_error() {
        let result = call("daily", json!({"since": "not-a-date"}));
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Date must be in YYYYMMDD format"));
    }

    #[test]
    fn notifications_produce_no_response() {
        assert!(handle_request(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &args()
        )
        .is_none());
    }
}
