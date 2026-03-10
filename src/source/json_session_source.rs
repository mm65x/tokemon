use std::borrow::Cow;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::timestamp;
use crate::types::Record;

/// Configuration trait for JSON session file sources.
///
/// Implement this with a zero-sized type to define a new source that
/// reads `{ "messages": [...] }` JSON files with per-message token counts.
pub trait JsonSessionSourceConfig: Send + Sync + 'static {
    const NAME: &'static str;
    const DISPLAY_NAME: &'static str;

    fn base_dir() -> PathBuf;

    /// Return all data files for this provider.
    fn discover_files(base_dir: &Path) -> Vec<PathBuf>;

    /// Message types to accept (matched case-insensitively).
    fn accepted_types() -> &'static [&'static str];

    /// Extract session ID from a file path. Default uses file stem.
    fn extract_session_id(path: &Path) -> Option<String> {
        timestamp::extract_session_id(path)
    }
}

pub struct JsonSessionSource<C: JsonSessionSourceConfig> {
    base_dir: PathBuf,
    _config: PhantomData<C>,
}

impl<C: JsonSessionSourceConfig> Default for JsonSessionSource<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: JsonSessionSourceConfig> JsonSessionSource<C> {
    pub fn new() -> Self {
        Self {
            base_dir: C::base_dir(),
            _config: PhantomData,
        }
    }
}

#[derive(Deserialize)]
struct SessionFile {
    messages: Option<Vec<SessionMessage>>,
}

#[derive(Deserialize)]
struct SessionMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    model: Option<String>,
    timestamp: Option<String>,
    tokens: Option<SessionTokens>,
}

#[derive(Deserialize)]
struct SessionTokens {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
    thoughts: Option<u64>,
}

impl<C: JsonSessionSourceConfig> super::Source for JsonSessionSource<C> {
    fn name(&self) -> &'static str {
        C::NAME
    }

    fn display_name(&self) -> &'static str {
        C::DISPLAY_NAME
    }

    fn data_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        C::discover_files(&self.base_dir)
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let content = fs::read_to_string(path).map_err(TokemonError::Io)?;
        let session: SessionFile =
            serde_json::from_str(&content).map_err(|e| TokemonError::JsonParse {
                file: path.display().to_string(),
                source: e,
            })?;

        let Some(messages) = session.messages else {
            return Ok(Vec::new());
        };

        let accepted = C::accepted_types();
        let session_id = C::extract_session_id(path);

        let entries = messages
            .into_iter()
            .filter(|msg| {
                let t = msg.msg_type.as_deref().unwrap_or("");
                accepted.iter().any(|a| t.eq_ignore_ascii_case(a))
            })
            .filter_map(|msg| {
                let tokens = msg.tokens?;
                let timestamp = msg
                    .timestamp
                    .as_deref()
                    .and_then(timestamp::parse_timestamp)?;

                Some(Record {
                    timestamp,
                    provider: Cow::Borrowed(C::NAME),
                    model: msg.model,
                    input_tokens: tokens.input.unwrap_or(0),
                    output_tokens: tokens.output.unwrap_or(0),
                    cache_read_tokens: tokens.cached.unwrap_or(0),
                    cache_creation_tokens: 0,
                    thinking_tokens: tokens.thoughts.unwrap_or(0),
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
