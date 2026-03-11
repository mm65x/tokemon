use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::timestamp;
use crate::types::Record;

/// Configuration trait for generic JSONL sources.
///
/// Implement this with a zero-sized type to define a new source:
/// ```ignore
/// pub struct KimiConfig;
/// impl JsonlSourceConfig for KimiConfig {
///     const NAME: &'static str = "kimi";
///     const DISPLAY_NAME: &'static str = "Kimi";
///     fn base_dir() -> PathBuf { paths::home_dir().join(".kimi/sessions") }
/// }
/// pub type KimiSource = JsonlSource<KimiConfig>;
/// ```
pub trait JsonlSourceConfig: Send + Sync + 'static {
    const NAME: &'static str;
    const DISPLAY_NAME: &'static str;
    const HAS_CACHE_TOKENS: bool = false;
    const HAS_REQUEST_IDS: bool = false;
    const MAX_DEPTH: usize = 5;
    fn base_dir() -> PathBuf;
}

/// Generic JSONL source that handles the common pattern of:
/// - Reading `**/*.jsonl` files from a base directory
/// - Parsing lines with `type`, `timestamp`, `model`, `usage` fields
/// - Filtering for "assistant" or "response" type lines
/// - Extracting token counts from usage data
pub struct JsonlSource<C: JsonlSourceConfig> {
    base_dir: PathBuf,
    _config: PhantomData<C>,
}

impl<C: JsonlSourceConfig> Default for JsonlSource<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: JsonlSourceConfig> JsonlSource<C> {
    pub fn new() -> Self {
        Self {
            base_dir: C::base_dir(),
            _config: PhantomData,
        }
    }
}

/// Unified deserialization struct for all JSONL providers.
/// Fields not present in the JSON are deserialized as None.
#[derive(Deserialize)]
struct JsonlLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    timestamp: Option<String>,
    model: Option<String>,
    usage: Option<JsonlUsage>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    #[serde(rename = "messageId")]
    message_id: Option<String>,
}

#[derive(Deserialize)]
struct JsonlUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_creation_tokens: Option<u64>,
}

impl<C: JsonlSourceConfig> super::Source for JsonlSource<C> {
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
        super::discover::walk_by_ext(&self.base_dir, "jsonl", C::MAX_DEPTH)
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let session_id = timestamp::extract_session_id(path);

        let mut error_logged = false;
        let entries = reader
            .lines()
            .map_while(std::result::Result::ok)
            .filter(|line| line.contains("\"assistant\"") || line.contains("\"response\""))
            .filter_map(|line| match serde_json::from_str::<JsonlLine>(&line) {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    if !error_logged {
                        eprintln!(
                            "[tokemon] Warning: skipped malformed JSON in {}: {}",
                            path.display(),
                            e
                        );
                        error_logged = true;
                    }
                    None
                }
            })
            .filter(|parsed| matches!(parsed.line_type.as_deref(), Some("assistant" | "response")))
            .filter_map(|parsed| {
                let usage = parsed.usage?;
                let timestamp = parsed
                    .timestamp
                    .as_deref()
                    .and_then(timestamp::parse_timestamp)?;

                Some(Record {
                    timestamp,
                    provider: Cow::Borrowed(C::NAME),
                    model: parsed.model,
                    input_tokens: usage.input_tokens.unwrap_or(0),
                    output_tokens: usage.output_tokens.unwrap_or(0),
                    cache_read_tokens: if C::HAS_CACHE_TOKENS {
                        usage.cache_read_tokens.unwrap_or(0)
                    } else {
                        0
                    },
                    cache_creation_tokens: if C::HAS_CACHE_TOKENS {
                        usage.cache_creation_tokens.unwrap_or(0)
                    } else {
                        0
                    },
                    thinking_tokens: 0,
                    cost_usd: None,
                    message_id: if C::HAS_REQUEST_IDS {
                        parsed.message_id
                    } else {
                        None
                    },
                    request_id: if C::HAS_REQUEST_IDS {
                        parsed.request_id
                    } else {
                        None
                    },
                    session_id: session_id.clone(),
                })
            })
            .collect();

        Ok(entries)
    }
}
