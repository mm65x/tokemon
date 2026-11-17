use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct AntigravitySource {
    base_dirs: Vec<PathBuf>,
}

impl Default for AntigravitySource {
    fn default() -> Self {
        Self::new()
    }
}

impl AntigravitySource {
    pub fn new() -> Self {
        let gemini = paths::home_dir().join(".gemini");
        Self {
            base_dirs: vec![
                gemini.join("antigravity").join("brain"),
                gemini.join("antigravity-cli").join("brain"),
            ],
        }
    }
}

/// Minimal deserialization struct for transcript JSONL lines.
/// Only model-response steps (`PLANNER_RESPONSE`) carry token counts.
#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    created_at: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
}

impl super::Source for AntigravitySource {
    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn display_name(&self) -> &'static str {
        "Antigravity"
    }

    fn data_dir(&self) -> PathBuf {
        // Report the primary data directory for the `discover` command.
        self.base_dirs.first().cloned().unwrap_or_default()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for base in &self.base_dirs {
            let Ok(brains) = fs::read_dir(base) else {
                continue;
            };
            for entry in brains.filter_map(std::result::Result::ok) {
                if !entry.path().is_dir() {
                    continue;
                }
                let transcript = entry
                    .path()
                    .join(".system_generated")
                    .join("logs")
                    .join("transcript.jsonl");
                if transcript.is_file() {
                    files.push(transcript);
                }
            }
        }
        files
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);

        // Extract session ID from the brain UUID in the path.
        // Layout: .../brain/{uuid}/.system_generated/logs/transcript.jsonl
        let session_id = path
            .ancestors()
            .nth(3)
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned());

        let entries = reader
            .lines()
            .map_while(std::result::Result::ok)
            .filter(|line| line.contains("\"PLANNER_RESPONSE\""))
            .filter_map(|line| serde_json::from_str::<TranscriptLine>(&line).ok())
            .filter(|parsed| matches!(parsed.line_type.as_deref(), Some("PLANNER_RESPONSE")))
            .filter_map(|parsed| {
                // Require at least one token count to exist.
                let input = parsed.input_tokens?;
                let timestamp = parsed
                    .created_at
                    .as_deref()
                    .and_then(timestamp::parse_timestamp)?;

                Some(Record {
                    timestamp,
                    provider: Cow::Borrowed("antigravity"),
                    model: None,
                    input_tokens: input,
                    output_tokens: parsed.output_tokens.unwrap_or(0),
                    cache_read_tokens: parsed.cache_read_tokens.unwrap_or(0),
                    cache_creation_tokens: 0,
                    thinking_tokens: 0,
                    cost_usd: None,
                    message_id: None,
                    request_id: None,
                    session_id: session_id.clone(),
                })
            })
            .collect();

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;

    fn write_transcript(root: &Path, id: &str, body: &str) -> PathBuf {
        let logs = root.join(id).join(".system_generated").join("logs");
        fs::create_dir_all(&logs).unwrap();
        let file = logs.join("transcript.jsonl");
        fs::write(&file, body).unwrap();
        file
    }

    #[test]
    fn parses_planner_responses_only() {
        let root = std::env::temp_dir().join("tokemon_antigravity_parse");
        let _ = fs::remove_dir_all(&root);
        let body = concat!(
            r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","created_at":"2026-01-01T00:00:00Z","content":"hi"}"#,
            "\n",
            r#"{"step_index":2,"source":"MODEL","type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:05Z","input_tokens":100,"cache_read_tokens":40,"output_tokens":7}"#,
            "\n",
            "not json PLANNER_RESPONSE\n",
            r#"{"step_index":3,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:09Z"}"#,
            "\n",
            r#"{"step_index":5,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:01:00Z","input_tokens":5,"output_tokens":1}"#,
            "\n",
        );
        let file = write_transcript(&root, "abc-123", body);

        let source = AntigravitySource {
            base_dirs: vec![root.clone()],
        };
        let records = source.parse_file(&file).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].cache_read_tokens, 40);
        assert_eq!(records[0].output_tokens, 7);
        assert_eq!(records[0].total_tokens(), 147);
        assert_eq!(records[0].session_id.as_deref(), Some("abc-123"));
        assert_eq!(records[1].cache_read_tokens, 0);
        assert!(records.iter().all(|r| r.provider == "antigravity"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discovers_transcripts_across_base_dirs() {
        let a = std::env::temp_dir().join("tokemon_antigravity_disc_a");
        let b = std::env::temp_dir().join("tokemon_antigravity_disc_b");
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
        write_transcript(&a, "one", "");
        write_transcript(&b, "two", "");
        fs::create_dir_all(a.join("empty-brain")).unwrap();

        let source = AntigravitySource {
            base_dirs: vec![a.clone(), b.clone(), PathBuf::from("/nonexistent/path")],
        };
        assert_eq!(source.discover_files().len(), 2);

        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }
}
