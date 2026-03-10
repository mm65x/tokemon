use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::types::Record;

/// Shared parsing logic for Cline-derived tools (Cline, Roo Code, Kilo Code)
pub struct ClineFormat {
    pub provider_name: &'static str,
    pub extension_id: &'static str,
}

#[derive(Deserialize)]
struct UiMessage {
    ts: Option<i64>,
    say: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiReqData {
    #[serde(rename = "tokensIn")]
    tokens_in: Option<u64>,
    #[serde(rename = "tokensOut")]
    tokens_out: Option<u64>,
    #[serde(rename = "cacheWrites")]
    cache_writes: Option<u64>,
    #[serde(rename = "cacheReads")]
    cache_reads: Option<u64>,
    cost: Option<f64>,
    model: Option<String>,
}

impl ClineFormat {
    pub fn discover_files(&self) -> Vec<PathBuf> {
        // Structure: {globalStorage}/{extension_id}/tasks/{task_id}/ui_messages.json
        let storage_dirs = paths::vscode_global_storage_dirs();
        let mut files = Vec::new();

        for storage_dir in storage_dirs {
            let tasks_dir = storage_dir.join(self.extension_id).join("tasks");
            let Ok(tasks) = std::fs::read_dir(&tasks_dir) else {
                continue;
            };
            for task in tasks.filter_map(std::result::Result::ok) {
                if !task.path().is_dir() {
                    continue;
                }
                let ui_file = task.path().join("ui_messages.json");
                if ui_file.is_file() {
                    files.push(ui_file);
                }
            }
        }
        files
    }

    pub fn data_dir(&self) -> PathBuf {
        let storage_dirs = paths::vscode_global_storage_dirs();
        if let Some(first) = storage_dirs.first() {
            first.join(self.extension_id)
        } else {
            PathBuf::from(format!("(VSCode globalStorage)/{}", self.extension_id))
        }
    }

    pub fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let content = fs::read_to_string(path).map_err(TokemonError::Io)?;
        let messages: Vec<UiMessage> =
            serde_json::from_str(&content).map_err(|e| TokemonError::JsonParse {
                file: path.display().to_string(),
                source: e,
            })?;

        // Extract session_id from parent directory name
        let session_id = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(std::string::ToString::to_string);

        let mut entries = Vec::new();

        for msg in messages {
            if msg.say.as_deref() != Some("api_req_started") {
                continue;
            }

            let Some(text) = &msg.text else { continue };

            let req_data: ApiReqData = match serde_json::from_str(text) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let Some(timestamp) = msg.ts.and_then(crate::timestamp::parse_timestamp_millis) else {
                continue;
            };

            entries.push(Record {
                timestamp,
                provider: Cow::Borrowed(self.provider_name),
                model: req_data.model,
                input_tokens: req_data.tokens_in.unwrap_or(0),
                output_tokens: req_data.tokens_out.unwrap_or(0),
                cache_read_tokens: req_data.cache_reads.unwrap_or(0),
                cache_creation_tokens: req_data.cache_writes.unwrap_or(0),
                thinking_tokens: 0,
                cost_usd: req_data.cost,
                message_id: None,
                request_id: None,
                session_id: session_id.clone(),
            });
        }

        Ok(entries)
    }
}
