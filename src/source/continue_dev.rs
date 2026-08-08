use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::timestamp;
use crate::types::Record;

pub struct ContinueSource {
    base_dir: PathBuf,
}

impl Default for ContinueSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinueSource {
    pub fn new() -> Self {
        Self {
            base_dir: paths::home_dir().join(".continue/dev_data"),
        }
    }
}

#[derive(Deserialize)]
struct TokenEvent {
    timestamp: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    #[serde(rename = "promptTokens")]
    prompt_tokens: Option<u64>,
    #[serde(rename = "generatedTokens")]
    generated_tokens: Option<u64>,
}

#[must_use]
fn provider_prefix(provider: &str) -> &str {
    match provider {
        "vertexai" | "google-vertex" => "vertexai.",
        "openai" => "openai/",
        "anthropic" => "anthropic/",
        "gemini" | "google" => "google/",
        "bedrock" | "aws-bedrock" => "bedrock/",
        "azure" | "azure-openai" => "azure/",
        _ => "",
    }
}

#[must_use]
fn qualify_model(model: String, provider: Option<&str>) -> String {
    let prefix = provider.map(provider_prefix).unwrap_or_default();
    if prefix.is_empty()
        || model.contains('/')
        || model.starts_with("vertexai.")
        || model.starts_with("anthropic.")
    {
        model
    } else {
        format!("{prefix}{model}")
    }
}

impl super::Source for ContinueSource {
    fn name(&self) -> &'static str {
        "continue"
    }

    fn display_name(&self) -> &'static str {
        "Continue"
    }

    fn data_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        let Ok(version_dirs) = fs::read_dir(&self.base_dir) else {
            return Vec::new();
        };
        let mut files = version_dirs
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(std::fs::FileType::is_dir)
                    .map(|_| entry.path().join("tokensGenerated.jsonl"))
            })
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let mut io_errors = 0u64;
        let mut json_errors = 0u64;
        let mut invalid_events = 0u64;
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    if io_errors == 0 {
                        eprintln!(
                            "[tokemon] Warning: I/O error reading {}: {error}",
                            path.display()
                        );
                    }
                    io_errors += 1;
                    continue;
                }
            };

            if !line.contains("\"promptTokens\"") || !line.contains("\"generatedTokens\"") {
                continue;
            }

            let Ok(event) = serde_json::from_str::<TokenEvent>(&line) else {
                json_errors += 1;
                continue;
            };

            let Some(parsed_timestamp) = event
                .timestamp
                .as_deref()
                .and_then(timestamp::parse_timestamp)
            else {
                invalid_events += 1;
                continue;
            };
            let (Some(prompt_tokens), Some(generated_tokens)) =
                (event.prompt_tokens, event.generated_tokens)
            else {
                invalid_events += 1;
                continue;
            };
            if prompt_tokens == 0 && generated_tokens == 0 {
                continue;
            }

            entries.push(Record {
                timestamp: parsed_timestamp,
                provider: Cow::Borrowed("continue"),
                model: event
                    .model
                    .map(|model| qualify_model(model, event.provider.as_deref())),
                input_tokens: prompt_tokens,
                output_tokens: generated_tokens,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                thinking_tokens: 0,
                cost_usd: None,
                message_id: None,
                request_id: None,
                session_id: None,
            });
        }

        if io_errors > 0 {
            eprintln!(
                "[tokemon] Warning: skipped {io_errors} lines in {} due to I/O errors",
                path.display()
            );
        }
        if json_errors > 0 {
            eprintln!(
                "[tokemon] Warning: skipped {json_errors} malformed JSON lines in {}",
                path.display()
            );
        }
        if invalid_events > 0 {
            eprintln!(
                "[tokemon] Warning: skipped {invalid_events} incomplete usage events in {}",
                path.display()
            );
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifies_known_api_providers() {
        assert_eq!(
            qualify_model("claude-sonnet-4".to_string(), Some("anthropic")),
            "anthropic/claude-sonnet-4"
        );
        assert_eq!(
            qualify_model("gemini-2.5-pro".to_string(), Some("vertexai")),
            "vertexai.gemini-2.5-pro"
        );
        assert_eq!(
            qualify_model("openai/gpt-4o".to_string(), Some("openai")),
            "openai/gpt-4o"
        );
        assert_eq!(
            qualify_model("custom-model".to_string(), Some("custom")),
            "custom-model"
        );
    }
}
