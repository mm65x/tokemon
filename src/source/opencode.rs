use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct OpenCodeSource {
    db_path: PathBuf,
}

impl Default for OpenCodeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenCodeSource {
    pub fn new() -> Self {
        Self {
            db_path: paths::home_dir().join(".local/share/opencode/opencode.db"),
        }
    }
}

/// Map an OpenCode `providerID` to a model-name prefix that
/// `display::infer_api_provider` already understands.
#[must_use]
fn provider_prefix(provider_id: &str) -> &str {
    match provider_id {
        "google-vertex" | "google-vertex-anthropic" => "vertexai.",
        "openai" => "openai/",
        "bedrock" | "aws-bedrock" => "bedrock/",
        "azure" | "azure-openai" => "azure/",
        _ => "", // anthropic, opencode, etc. — model name alone is sufficient
    }
}

impl super::Source for OpenCodeSource {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn display_name(&self) -> &'static str {
        "OpenCode"
    }

    fn data_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        if self.db_path.exists() {
            vec![self.db_path.clone()]
        } else {
            Vec::new()
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let conn = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        // Wait up to 5s if the DB is locked by a running OpenCode process
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));

        let mut stmt = conn.prepare(
            "SELECT
                m.id, m.session_id, m.time_created,
                json_extract(m.data, '$.modelID'),
                json_extract(m.data, '$.providerID'),
                json_extract(m.data, '$.cost'),
                json_extract(m.data, '$.tokens.input'),
                json_extract(m.data, '$.tokens.output'),
                json_extract(m.data, '$.tokens.reasoning'),
                json_extract(m.data, '$.tokens.cache.read'),
                json_extract(m.data, '$.tokens.cache.write')
             FROM message m
             WHERE json_extract(m.data, '$.role') = 'assistant'
               AND (json_extract(m.data, '$.tokens.input') > 0
                    OR json_extract(m.data, '$.tokens.output') > 0)
             ORDER BY m.time_created",
        )?;

        let entries = stmt
            .query_map([], |row| {
                let _msg_id: String = row.get(0)?;
                let session_id: String = row.get(1)?;
                let time_created: i64 = row.get(2)?;
                let model_id: Option<String> = row.get(3)?;
                let provider_id: Option<String> = row.get(4)?;
                let cost: Option<f64> = row.get(5)?;
                let input_tokens: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
                let output_tokens: i64 = row.get::<_, Option<i64>>(7)?.unwrap_or(0);
                let reasoning_tokens: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(0);
                let cache_read: i64 = row.get::<_, Option<i64>>(9)?.unwrap_or(0);
                let cache_write: i64 = row.get::<_, Option<i64>>(10)?.unwrap_or(0);
                Ok((
                    session_id,
                    time_created,
                    model_id,
                    provider_id,
                    cost,
                    input_tokens,
                    output_tokens,
                    reasoning_tokens,
                    cache_read,
                    cache_write,
                ))
            })
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let (
                    session_id,
                    time_created,
                    model_id,
                    provider_id,
                    cost,
                    input_tokens,
                    output_tokens,
                    reasoning_tokens,
                    cache_read,
                    cache_write,
                ) = row.ok()?;

                let ts = timestamp::parse_timestamp_numeric(time_created)?;

                // Strip @... suffix (e.g. "claude-opus-4-6@default" → "claude-opus-4-6")
                let model_raw = model_id.as_deref().unwrap_or("unknown");
                let model_clean = model_raw.split('@').next().unwrap_or(model_raw);

                // Prefix model with provider hint for infer_api_provider
                let prefix = provider_prefix(provider_id.as_deref().unwrap_or(""));
                let model = if prefix.is_empty() {
                    model_clean.to_string()
                } else {
                    format!("{prefix}{model_clean}")
                };

                Some(Record {
                    timestamp: ts,
                    provider: Cow::Borrowed("opencode"),
                    model: Some(model),
                    input_tokens: input_tokens.max(0) as u64,
                    output_tokens: output_tokens.max(0) as u64,
                    cache_read_tokens: cache_read.max(0) as u64,
                    cache_creation_tokens: cache_write.max(0) as u64,
                    thinking_tokens: reasoning_tokens.max(0) as u64,
                    cost_usd: cost,
                    message_id: None,
                    request_id: None,
                    session_id: Some(session_id),
                })
            })
            .collect();

        Ok(entries)
    }
}
