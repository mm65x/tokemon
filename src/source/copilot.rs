use std::borrow::Cow;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct CopilotSource;

impl Default for CopilotSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CopilotSource {
    pub fn new() -> Self {
        Self
    }
}

impl super::Source for CopilotSource {
    fn name(&self) -> &'static str {
        "copilot"
    }

    fn display_name(&self) -> &'static str {
        "GitHub Copilot"
    }

    fn data_dir(&self) -> PathBuf {
        let storage_dirs = paths::vscode_global_storage_dirs();
        storage_dirs.first().map_or_else(
            || PathBuf::from("(editor telemetry)"),
            std::clone::Clone::clone,
        )
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for root in telemetry_roots() {
            files.extend(super::discover::walk_by_ext(&root, "jsonl", 4));
        }
        files.sort();
        files.dedup();
        files
    }

    #[allow(clippy::too_many_lines)]
    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = File::open(path)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let session_id = timestamp::extract_session_id(path);
        let mut malformed_logged = false;
        let mut incomplete = 0usize;
        let mut records = Vec::new();

        for line in reader.lines().map_while(std::result::Result::ok) {
            if !line.contains("input_tokens") && !line.contains("prompt_tokens") {
                continue;
            }
            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(error) => {
                    if !malformed_logged {
                        eprintln!(
                            "[tokemon] Warning: skipped malformed telemetry JSON in {}: {}",
                            path.display(),
                            error
                        );
                        malformed_logged = true;
                    }
                    continue;
                }
            };

            let Some(timestamp) = timestamp_value(&value) else {
                incomplete += 1;
                continue;
            };
            let input_tokens = first_u64(
                &value,
                &[
                    "gen_ai.usage.input_tokens",
                    "gen_ai.usage.prompt_tokens",
                    "input_tokens",
                    "prompt_tokens",
                ],
            );
            let output_tokens = first_u64(
                &value,
                &[
                    "gen_ai.usage.output_tokens",
                    "gen_ai.usage.completion_tokens",
                    "output_tokens",
                    "completion_tokens",
                ],
            );
            let cache_read_tokens = first_u64(
                &value,
                &[
                    "gen_ai.usage.cache_read_input_tokens",
                    "gen_ai.usage.cache_read_tokens",
                    "cache_read_input_tokens",
                    "cache_read_tokens",
                ],
            );
            let cache_creation_tokens = first_u64(
                &value,
                &[
                    "gen_ai.usage.cache_creation_input_tokens",
                    "gen_ai.usage.cache_write_input_tokens",
                    "gen_ai.usage.cache_creation_tokens",
                    "cache_creation_tokens",
                ],
            );
            let thinking_tokens = first_u64(
                &value,
                &["gen_ai.usage.reasoning_tokens", "reasoning_tokens"],
            );
            if input_tokens
                + output_tokens
                + cache_read_tokens
                + cache_creation_tokens
                + thinking_tokens
                == 0
            {
                incomplete += 1;
                continue;
            }

            let model = first_string(
                &value,
                &["gen_ai.request.model", "gen_ai.response.model", "model"],
            );
            let message_id = first_string(&value, &["message_id", "messageId"]);
            let request_id =
                first_string(&value, &["gen_ai.request.id", "request_id", "requestId"]);
            let record_session = first_string(
                &value,
                &["session_id", "sessionId", "copilot_chat.session_id"],
            )
            .or_else(|| session_id.clone());

            records.push(Record {
                timestamp,
                provider: Cow::Borrowed("copilot"),
                model,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                thinking_tokens,
                cost_usd: None,
                message_id,
                request_id,
                session_id: record_session,
            });
        }

        if incomplete > 0 {
            eprintln!(
                "[tokemon] Warning: skipped {incomplete} incomplete telemetry records in {}",
                path.display()
            );
        }
        Ok(records)
    }
}

fn telemetry_roots() -> Vec<PathBuf> {
    paths::vscode_global_storage_dirs()
        .into_iter()
        .flat_map(|root| {
            ["github.copilot-chat", "github.copilot"]
                .into_iter()
                .map(move |extension| root.join(extension))
        })
        .filter(|root| root.is_dir())
        .collect()
}

fn value_at<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.')
        .try_fold(value, |current, key| current.get(key))
}

fn first_string(value: &serde_json::Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        value_at(value, path)
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn first_u64(value: &serde_json::Value, paths: &[&str]) -> u64 {
    paths
        .iter()
        .find_map(|path| {
            value_at(value, path).and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|raw| raw.parse().ok()))
            })
        })
        .unwrap_or(0)
}

fn timestamp_value(value: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    for path in ["timestamp", "time", "time_unix_nano", "timeUnixNano"] {
        let Some(candidate) = value_at(value, path) else {
            continue;
        };
        if let Some(raw) = candidate.as_str() {
            if let Some(parsed) = timestamp::parse_timestamp(raw) {
                return Some(parsed);
            }
        }
        if let Some(raw) = candidate.as_i64() {
            if raw > 1_000_000_000_000_000 {
                return chrono::DateTime::from_timestamp_millis(raw / 1_000_000);
            }
            if let Some(parsed) = timestamp::parse_timestamp_numeric(raw) {
                return Some(parsed);
            }
        }
        if let Some(raw) = candidate.as_u64() {
            if raw > 1_000_000_000_000_000 {
                return chrono::DateTime::from_timestamp_millis((raw / 1_000_000) as i64);
            }
            if let Some(parsed) = timestamp::parse_timestamp(&raw.to_string()) {
                return Some(parsed);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::CopilotSource;
    use crate::source::Source;
    use std::io::Write;

    #[test]
    fn parses_nested_telemetry_and_skips_incomplete_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"timestamp":"2026-02-20T10:00:00Z","session_id":"s-1","gen_ai":{{"request":{{"model":"model-x","id":"req-1"}},"usage":{{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}}}}}"#
        )
        .unwrap();
        writeln!(file, "{{\"input_tokens\":10}}").unwrap();
        writeln!(file, "not json").unwrap();

        let records = CopilotSource::new().parse_file(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].input_tokens, 10);
        assert_eq!(records[0].output_tokens, 20);
        assert_eq!(records[0].cache_read_tokens, 5);
        assert_eq!(records[0].model.as_deref(), Some("model-x"));
        assert_eq!(records[0].session_id.as_deref(), Some("s-1"));
        assert_eq!(records[0].request_id.as_deref(), Some("req-1"));
    }
}
