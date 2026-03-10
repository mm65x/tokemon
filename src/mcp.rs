//! Minimal MCP (Model Context Protocol) server over stdio.
//!
//! Implements the JSON-RPC 2.0 subset needed for MCP:
//! - `initialize` / `initialized`
//! - `tools/list`
//! - `tools/call`

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::cli::Cli;
use crate::config::Config;
use crate::rollup;
use crate::types::SessionReport;

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub fn run(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                writeln!(stdout, "{err}")?;
                stdout.flush()?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(&id),
            "notifications/initialized" | "initialized" => {
                // Notification, no response needed
                continue;
            }
            "tools/list" => handle_tools_list(&id),
            "tools/call" => handle_tools_call(&id, &request, cli, config),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }),
        };

        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_initialize(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "tokemon",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn handle_tools_list(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "get_usage_today",
                    "description": "Get today's total token usage and cost across all providers",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_usage_period",
                    "description": "Get token usage for a date range",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "since": {
                                "type": "string",
                                "description": "Start date (YYYY-MM-DD)"
                            },
                            "until": {
                                "type": "string",
                                "description": "End date (YYYY-MM-DD)"
                            },
                            "period": {
                                "type": "string",
                                "enum": ["daily", "weekly", "monthly"],
                                "description": "Aggregation period (default: daily)"
                            }
                        }
                    }
                },
                {
                    "name": "get_budget_status",
                    "description": "Get current spend vs configured budget limits",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_session_cost",
                    "description": "Get cost and token usage for a specific session",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "session_id": {
                                "type": "string",
                                "description": "Session ID (full or prefix)"
                            }
                        },
                        "required": ["session_id"]
                    }
                }
            ]
        }
    })
}

fn handle_tools_call(id: &Value, request: &Value, cli: &Cli, config: &Config) -> Value {
    let params = request.get("params").cloned().unwrap_or(json!({}));
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match tool_name {
        "get_usage_today" => tool_usage_today(cli, config),
        "get_usage_period" => tool_usage_period(cli, config, &arguments),
        "get_budget_status" => tool_budget_status(cli, config),
        "get_session_cost" => tool_session_cost(cli, config, &arguments),
        _ => Err(format!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(content) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{
                    "type": "text",
                    "text": content
                }]
            }
        }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{
                    "type": "text",
                    "text": format!("Error: {}", e)
                }],
                "isError": true
            }
        }),
    }
}

// MCP tool handlers use `Result<String, String>` intentionally: the MCP
// protocol returns errors as plain-text `"isError": true` content blocks,
// so structured error types would be immediately serialized to strings
// anyway. Converting via `.map_err(|e| e.to_string())` is the correct
// boundary between internal `anyhow::Error` and the MCP wire format.
fn tool_usage_today(cli: &Cli, config: &Config) -> Result<String, String> {
    let today = chrono::Utc::now().date_naive();
    let entries = crate::pipeline::load_and_price(cli, config, true, Some(today), None)
        .map_err(|e| e.to_string())?;
    let mut total_tokens = 0u64;
    let mut total_cost = 0.0f64;

    for e in &entries {
        if e.timestamp.date_naive() == today {
            total_tokens += e.total_tokens();
            total_cost += e.cost_usd.unwrap_or(0.0);
        }
    }

    let result = json!({
        "date": today.to_string(),
        "total_tokens": total_tokens,
        "cost_usd": (total_cost * 100.0).round() / 100.0
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

fn tool_usage_period(cli: &Cli, config: &Config, args: &Value) -> Result<String, String> {
    let since = args
        .get("since")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<chrono::NaiveDate>().ok());
    let until = args
        .get("until")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<chrono::NaiveDate>().ok());

    let entries = crate::pipeline::load_and_price(cli, config, true, since, until)
        .map_err(|e| e.to_string())?;

    let entries = rollup::filter_by_date(entries, since, until);

    let period = args
        .get("period")
        .and_then(|v| v.as_str())
        .unwrap_or("daily");

    let summaries = match period {
        "weekly" => rollup::aggregate_weekly(&entries),
        "monthly" => rollup::aggregate_monthly(&entries),
        _ => rollup::aggregate_daily(&entries),
    };

    let total_cost: f64 = summaries.iter().map(|s| s.total_cost).sum();
    let total_tokens: u64 = entries.iter().map(super::types::Record::total_tokens).sum();

    let report = crate::types::Report {
        period: period.to_string(),
        generated_at: chrono::Utc::now(),
        providers_found: Vec::new(),
        summaries,
        total_cost,
        total_tokens,
    };

    serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
}

fn tool_budget_status(cli: &Cli, config: &Config) -> Result<String, String> {
    let entries = crate::pipeline::load_and_price(cli, config, true, None, None)
        .map_err(|e| e.to_string())?;
    let status = crate::pacemaker::evaluate(&entries, &config.budget);

    let to_json = |bp: &crate::pacemaker::BudgetPeriod| {
        let pct = if bp.limit > 0.0 {
            (bp.spent / bp.limit * 100.0).round()
        } else {
            0.0
        };
        json!({"spent": (bp.spent * 100.0).round() / 100.0, "limit": bp.limit, "percent": pct})
    };
    let result = json!({
        "daily": status.daily.as_ref().map(to_json),
        "weekly": status.weekly.as_ref().map(to_json),
        "monthly": status.monthly.as_ref().map(to_json),
    });

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

fn tool_session_cost(cli: &Cli, config: &Config, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "session_id is required".to_string())?;

    let entries = crate::pipeline::load_and_price(cli, config, true, None, None)
        .map_err(|e| e.to_string())?;
    let sessions = rollup::aggregate_by_session(&entries);

    let matched: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.session_id.starts_with(session_id))
        .collect();

    if matched.is_empty() {
        return Err(format!("No session found matching '{session_id}'"));
    }

    let total_cost: f64 = matched.iter().map(|s| s.cost).sum();
    let total_tokens: u64 = matched.iter().map(|s| s.total_tokens).sum();

    let report = SessionReport {
        generated_at: chrono::Utc::now(),
        sessions: matched,
        total_cost,
        total_tokens,
    };

    serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
}
