use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct CodexSource {
    base_dir: PathBuf,
}

impl Default for CodexSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexSource {
    pub fn new() -> Self {
        Self {
            base_dir: paths::home_dir().join(".codex/sessions"),
        }
    }
}

#[derive(Deserialize)]
struct CodexLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    timestamp: Option<String>,
    payload: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct TokenUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
}

impl super::Source for CodexSource {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex CLI"
    }

    fn data_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        // Structure: sessions/YYYY/MM/DD/rollout-*.jsonl (depth 4)
        super::discover::walk_by_ext(&self.base_dir, "jsonl", 4)
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let mut entries = Vec::new();

        let session_id = timestamp::extract_session_id(path);

        // State machine: track current model from turn_context lines
        let mut current_model: Option<String> = None;

        for line in reader.lines() {
            let Ok(line) = line else {
                continue;
            };

            // Pre-filter: skip lines that are neither turn_context nor event_msg
            if !(line.contains("\"turn_context\"") || line.contains("\"event_msg\"")) {
                continue;
            }

            let Ok(parsed) = serde_json::from_str::<CodexLine>(&line) else {
                continue;
            };

            let line_type = match &parsed.line_type {
                Some(t) => t.as_str(),
                None => continue,
            };

            match line_type {
                "turn_context" => {
                    // Extract model from payload
                    if let Some(payload) = &parsed.payload {
                        if let Some(model) = payload.get("model").and_then(|m| m.as_str()) {
                            current_model = Some(model.to_string());
                        }
                    }
                }
                "event_msg" => {
                    let Some(payload) = &parsed.payload else {
                        continue;
                    };

                    // Check if this is a token_count event
                    let payload_type = payload.get("type").and_then(|t| t.as_str());
                    if payload_type != Some("token_count") {
                        continue;
                    }

                    let Some(info) = payload.get("info") else {
                        continue;
                    };

                    // Try last_token_usage first, then total_token_usage
                    let usage_val = info
                        .get("last_token_usage")
                        .or_else(|| info.get("total_token_usage"));

                    let usage: TokenUsage = match usage_val {
                        Some(v) => match serde_json::from_value(v.clone()) {
                            Ok(u) => u,
                            Err(_) => continue,
                        },
                        None => continue,
                    };

                    let Some(timestamp) = parsed
                        .timestamp
                        .as_deref()
                        .and_then(timestamp::parse_timestamp)
                    else {
                        continue;
                    };

                    let raw_input = usage.input_tokens.unwrap_or(0);
                    let cached = usage.cached_input_tokens.unwrap_or(0);
                    // Codex input_tokens includes cached tokens
                    let actual_input = if cached > raw_input {
                        eprintln!(
                            "[tokemon] Warning: cached tokens ({}) > input tokens ({}) in {}",
                            cached,
                            raw_input,
                            path.display()
                        );
                        0
                    } else {
                        raw_input - cached
                    };

                    entries.push(Record {
                        timestamp,
                        provider: Cow::Borrowed("codex"),
                        model: current_model.clone(),
                        input_tokens: actual_input,
                        output_tokens: usage.output_tokens.unwrap_or(0),
                        cache_read_tokens: cached,
                        cache_creation_tokens: 0,
                        thinking_tokens: 0,
                        cost_usd: None,
                        message_id: None,
                        request_id: None,
                        session_id: session_id.clone(),
                    });
                }
                _ => {}
            }
        }

        Ok(entries)
    }
}
