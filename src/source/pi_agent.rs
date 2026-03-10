use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct PiAgentSource {
    base_dir: PathBuf,
}

impl Default for PiAgentSource {
    fn default() -> Self {
        Self::new()
    }
}

impl PiAgentSource {
    pub fn new() -> Self {
        Self {
            base_dir: paths::home_dir().join(".pi/agent/sessions"),
        }
    }
}

#[derive(Deserialize)]
struct PiLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    timestamp: Option<String>,
    message: Option<PiMessage>,
}

#[derive(Deserialize)]
struct PiMessage {
    role: Option<String>,
    model: Option<String>,
    usage: Option<PiUsage>,
}

#[derive(Deserialize)]
struct PiUsage {
    input: Option<u64>,
    output: Option<u64>,
    #[serde(rename = "cacheRead")]
    cache_read: Option<u64>,
    #[serde(rename = "cacheWrite")]
    cache_write: Option<u64>,
}

impl super::Source for PiAgentSource {
    fn name(&self) -> &'static str {
        "pi-agent"
    }

    fn display_name(&self) -> &'static str {
        "Pi Agent"
    }

    fn data_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        // Structure: sessions/{project}/*.jsonl (depth 2)
        super::discover::walk_by_ext(&self.base_dir, "jsonl", 2)
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let session_id = timestamp::extract_session_id(path);

        let entries = reader
            .lines()
            .map_while(std::result::Result::ok)
            .filter(|line| line.contains("\"message\"") && line.contains("\"assistant\""))
            .filter_map(|line| serde_json::from_str::<PiLine>(&line).ok())
            .filter(|parsed| parsed.line_type.as_deref() == Some("message"))
            .filter_map(|parsed| {
                let message = parsed.message?;
                if message.role.as_deref() != Some("assistant") {
                    return None;
                }
                let usage = message.usage?;
                let ts = parsed
                    .timestamp
                    .as_deref()
                    .and_then(timestamp::parse_timestamp)?;

                Some(Record {
                    timestamp: ts,
                    provider: Cow::Borrowed("pi-agent"),
                    model: message.model,
                    input_tokens: usage.input.unwrap_or(0),
                    output_tokens: usage.output.unwrap_or(0),
                    cache_read_tokens: usage.cache_read.unwrap_or(0),
                    cache_creation_tokens: usage.cache_write.unwrap_or(0),
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
