use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths;
use crate::timestamp;
use crate::types::Record;

const DEFINITION_DIR: &str = "sources";
const MAX_DEPTH: usize = 8;

/// Versioned, data-only definition for a local JSONL source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomSourceDefinition {
    pub schema_version: u32,
    pub name: String,
    pub display_name: Option<String>,
    pub roots: Vec<String>,
    pub extension: String,
    pub max_depth: usize,
    pub format: String,
    pub provider: Option<String>,
    pub model_prefix: String,
    pub timestamp: String,
    pub model: String,
    pub input_tokens: String,
    pub output_tokens: String,
    pub cache_read_tokens: String,
    pub cache_creation_tokens: String,
    pub thinking_tokens: String,
    pub session_id: String,
    pub message_id: String,
    pub request_id: String,
}

impl Default for CustomSourceDefinition {
    fn default() -> Self {
        Self {
            schema_version: 1,
            name: String::new(),
            display_name: None,
            roots: Vec::new(),
            extension: "jsonl".to_string(),
            max_depth: 3,
            format: "jsonl".to_string(),
            provider: None,
            model_prefix: String::new(),
            timestamp: "timestamp".to_string(),
            model: "model".to_string(),
            input_tokens: "usage.input_tokens".to_string(),
            output_tokens: "usage.output_tokens".to_string(),
            cache_read_tokens: String::new(),
            cache_creation_tokens: String::new(),
            thinking_tokens: String::new(),
            session_id: String::new(),
            message_id: String::new(),
            request_id: String::new(),
        }
    }
}

/// A validated custom source loaded from a local definition.
pub struct CustomSource {
    definition: CustomSourceDefinition,
    roots: Vec<PathBuf>,
    name: &'static str,
    display_name: &'static str,
}

impl CustomSource {
    /// Build a source after validating its data-only definition.
    pub fn from_definition(definition: CustomSourceDefinition) -> anyhow::Result<Self> {
        validate_definition(&definition)?;
        let roots = definition
            .roots
            .iter()
            .map(|root| expand_root(root))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let name = leak_string(definition.name.clone());
        let display_name = leak_string(
            definition
                .display_name
                .clone()
                .unwrap_or_else(|| definition.name.clone()),
        );
        Ok(Self {
            definition,
            roots,
            name,
            display_name,
        })
    }

    /// Load and validate one TOML definition file.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let definition = toml::from_str(&content)?;
        Self::from_definition(definition)
    }

    /// Return the validated definition used by this source.
    #[must_use]
    pub const fn definition(&self) -> &CustomSourceDefinition {
        &self.definition
    }

    /// Return all locally configured custom sources, skipping invalid files.
    #[must_use]
    pub fn load_configured() -> Vec<Self> {
        let definition_dir = paths::config_dir().join(DEFINITION_DIR);
        crate::source::discover::walk_by_ext(&definition_dir, "toml", 2)
            .into_iter()
            .filter_map(|path| match Self::from_file(&path) {
                Ok(source) => Some(source),
                Err(error) => {
                    eprintln!(
                        "[tokemon] Warning: invalid custom source {}: {error}",
                        path.display()
                    );
                    None
                }
            })
            .collect()
    }

    fn parse_jsonl(&self, path: &Path) -> std::io::Result<Vec<Record>> {
        let file = fs::File::open(path)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        let session_from_path = timestamp::extract_session_id(path);
        let mut skipped = 0u64;
        let mut records = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    skipped += 1;
                    eprintln!(
                        "[tokemon] Warning: I/O error reading {}: {error}",
                        path.display()
                    );
                    continue;
                }
            };
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                skipped += 1;
                continue;
            };
            match self.record_from_value(&value, session_from_path.clone()) {
                Some(record) => records.push(record),
                None => skipped += 1,
            }
        }

        if skipped > 0 {
            eprintln!(
                "[tokemon] Warning: skipped {skipped} custom-source records in {}",
                path.display()
            );
        }
        Ok(records)
    }

    fn record_from_value(
        &self,
        value: &Value,
        session_from_path: Option<String>,
    ) -> Option<Record> {
        let timestamp = value_at_path(value, &self.definition.timestamp)
            .and_then(value_as_string)
            .and_then(|raw| timestamp::parse_timestamp(&raw))?;
        let model = value_at_path(value, &self.definition.model)
            .and_then(value_as_string)
            .map(|model| {
                if self.definition.model_prefix.is_empty() {
                    model
                } else {
                    format!("{}{model}", self.definition.model_prefix)
                }
            });
        let provider = self
            .definition
            .provider
            .as_deref()
            .unwrap_or(self.name)
            .to_string();

        Some(Record {
            timestamp,
            provider: Cow::Owned(provider),
            model,
            input_tokens: mapped_u64(value, &self.definition.input_tokens),
            output_tokens: mapped_u64(value, &self.definition.output_tokens),
            cache_read_tokens: mapped_u64(value, &self.definition.cache_read_tokens),
            cache_creation_tokens: mapped_u64(value, &self.definition.cache_creation_tokens),
            thinking_tokens: mapped_u64(value, &self.definition.thinking_tokens),
            cost_usd: None,
            message_id: mapped_string(value, &self.definition.message_id),
            request_id: mapped_string(value, &self.definition.request_id),
            session_id: mapped_string(value, &self.definition.session_id).or(session_from_path),
        })
    }
}

impl super::Source for CustomSource {
    fn name(&self) -> &'static str {
        self.name
    }

    fn display_name(&self) -> &'static str {
        self.display_name
    }

    fn data_dir(&self) -> PathBuf {
        self.roots.first().cloned().unwrap_or_default()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .flat_map(|root| {
                super::discover::walk_by_ext(
                    root,
                    self.definition.extension.trim_start_matches('.'),
                    self.definition.max_depth,
                )
            })
            .collect()
    }

    fn parse_file(&self, path: &Path) -> crate::error::Result<Vec<Record>> {
        self.parse_jsonl(path)
            .map_err(crate::error::TokemonError::Io)
    }
}

fn validate_definition(definition: &CustomSourceDefinition) -> anyhow::Result<()> {
    anyhow::ensure!(
        definition.schema_version == 1,
        "unsupported custom source schema version {}; expected 1",
        definition.schema_version
    );
    anyhow::ensure!(
        is_safe_name(&definition.name),
        "name must contain only letters, numbers, '.', '_' or '-'"
    );
    anyhow::ensure!(
        !definition.roots.is_empty(),
        "at least one root path is required"
    );
    anyhow::ensure!(
        definition.format.eq_ignore_ascii_case("jsonl"),
        "format must be jsonl"
    );
    let extension = definition.extension.trim_start_matches('.');
    anyhow::ensure!(
        !extension.is_empty() && extension.chars().all(|c| c.is_ascii_alphanumeric()),
        "extension must be a simple file extension"
    );
    anyhow::ensure!(
        (1..=MAX_DEPTH).contains(&definition.max_depth),
        "max_depth must be between 1 and {MAX_DEPTH}"
    );
    anyhow::ensure!(
        !definition.timestamp.trim().is_empty(),
        "timestamp mapping is required"
    );
    for (label, mapping) in [
        ("timestamp", definition.timestamp.as_str()),
        ("model", definition.model.as_str()),
        ("input_tokens", definition.input_tokens.as_str()),
        ("output_tokens", definition.output_tokens.as_str()),
        ("cache_read_tokens", definition.cache_read_tokens.as_str()),
        (
            "cache_creation_tokens",
            definition.cache_creation_tokens.as_str(),
        ),
        ("thinking_tokens", definition.thinking_tokens.as_str()),
        ("session_id", definition.session_id.as_str()),
        ("message_id", definition.message_id.as_str()),
        ("request_id", definition.request_id.as_str()),
    ] {
        anyhow::ensure!(
            valid_mapping(mapping),
            "{label} mapping must contain only dotted JSON object keys"
        );
    }
    anyhow::ensure!(
        !definition.input_tokens.trim().is_empty()
            || !definition.output_tokens.trim().is_empty()
            || !definition.cache_read_tokens.trim().is_empty()
            || !definition.cache_creation_tokens.trim().is_empty()
            || !definition.thinking_tokens.trim().is_empty(),
        "at least one token mapping is required"
    );
    for root in &definition.roots {
        let expanded = expand_root(root)?;
        anyhow::ensure!(
            expanded.is_absolute(),
            "root paths must be absolute or start with '~/'"
        );
    }
    Ok(())
}

fn expand_root(root: &str) -> anyhow::Result<PathBuf> {
    let root = root.trim();
    anyhow::ensure!(!root.is_empty(), "root paths cannot be empty");
    if root == "~" || root.starts_with("~/") {
        Ok(paths::home_dir().join(root.strip_prefix("~/").unwrap_or("")))
    } else {
        Ok(PathBuf::from(root))
    }
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return None;
    }
    path.split('.')
        .try_fold(value, |current, key| current.get(key))
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn mapped_string(value: &Value, path: &str) -> Option<String> {
    value_at_path(value, path).and_then(value_as_string)
}

fn mapped_u64(value: &Value, path: &str) -> u64 {
    value_at_path(value, path)
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(string) => string.parse().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
}

fn valid_mapping(mapping: &str) -> bool {
    mapping.is_empty()
        || mapping.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
        })
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_nested_jsonl_mappings_and_skips_bad_lines() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let file = directory.path().join("events.jsonl");
        fs::write(
            &file,
            "{\"at\":\"2026-08-09T10:00:00Z\",\"model\":\"model-a\",\"usage\":{\"in\":12,\"out\":8}}\nnot-json\n",
        )
        .expect("fixture write");
        let source = CustomSource::from_definition(CustomSourceDefinition {
            name: "private".to_string(),
            roots: vec![directory.path().display().to_string()],
            timestamp: "at".to_string(),
            model: "model".to_string(),
            input_tokens: "usage.in".to_string(),
            output_tokens: "usage.out".to_string(),
            ..CustomSourceDefinition::default()
        })
        .expect("valid definition");

        let records = source.parse_jsonl(&file).expect("parse succeeds");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].provider, "private");
        assert_eq!(records[0].total_tokens(), 20);
        assert_eq!(records[0].session_id.as_deref(), Some("events"));
    }

    #[test]
    fn rejects_future_schema_and_unbounded_paths() {
        let future = CustomSourceDefinition {
            schema_version: 2,
            roots: vec!["/tmp".to_string()],
            ..CustomSourceDefinition::default()
        };
        assert!(CustomSource::from_definition(future).is_err());

        let unbounded = CustomSourceDefinition {
            name: "private".to_string(),
            roots: vec!["/tmp".to_string()],
            max_depth: MAX_DEPTH + 1,
            ..CustomSourceDefinition::default()
        };
        assert!(CustomSource::from_definition(unbounded).is_err());
    }
}
