use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct ClaudeCodeSource {
    base_dir: PathBuf,
}

impl Default for ClaudeCodeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCodeSource {
    pub fn new() -> Self {
        Self {
            base_dir: paths::home_dir().join(".claude/projects"),
        }
    }
}

#[derive(Deserialize)]
struct ClaudeLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<ClaudeMessage>,
}

#[derive(Deserialize)]
struct ClaudeMessage {
    model: Option<String>,
    id: Option<String>,
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

impl super::Source for ClaudeCodeSource {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn data_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        // Structure: projects/{project}/{uuid}.jsonl        (session transcripts)
        //            projects/{project}/{uuid}/subagents/agent-{id}.jsonl
        let mut files = Vec::new();
        let Ok(projects) = fs::read_dir(&self.base_dir) else {
            return files;
        };
        for project in projects.filter_map(std::result::Result::ok) {
            let project_path = project.path();
            if !project_path.is_dir() {
                continue;
            }
            let Ok(entries) = fs::read_dir(&project_path) else {
                continue;
            };
            for entry in entries.filter_map(std::result::Result::ok) {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
                    files.push(path);
                } else if path.is_dir() {
                    // Check for subagents/ directory inside session UUID dirs
                    let subagents = path.join("subagents");
                    if subagents.is_dir() {
                        files.extend(super::discover::collect_by_ext(&subagents, "jsonl"));
                    }
                }
            }
        }
        files
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let session_id = timestamp::extract_session_id(path);

        let entries = reader
            .lines()
            .map_while(std::result::Result::ok)
            .filter(|line| line.contains("\"assistant\""))
            .filter_map(|line| serde_json::from_str::<ClaudeLine>(&line).ok())
            .filter(|parsed| parsed.line_type.as_deref() == Some("assistant"))
            .filter_map(|parsed| {
                let message = parsed.message?;
                if message.model.as_deref() == Some("<synthetic>") {
                    return None;
                }
                let usage = message.usage?;
                let timestamp = parsed
                    .timestamp
                    .as_deref()
                    .and_then(timestamp::parse_timestamp)?;

                // Detect Vertex AI from message ID prefix
                let model = match message.model {
                    Some(m)
                        if message
                            .id
                            .as_deref()
                            .is_some_and(|id| id.starts_with("msg_vrtx_")) =>
                    {
                        Some(format!("vertexai.{m}"))
                    }
                    other => other,
                };

                Some(Record {
                    timestamp,
                    provider: Cow::Borrowed("claude-code"),
                    model,
                    input_tokens: usage.input_tokens.unwrap_or(0),
                    output_tokens: usage.output_tokens.unwrap_or(0),
                    cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
                    cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
                    thinking_tokens: 0,
                    cost_usd: None,
                    message_id: message.id,
                    request_id: parsed.request_id,
                    session_id: session_id.clone(),
                })
            })
            .collect();

        Ok(entries)
    }
}
