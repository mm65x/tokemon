use std::borrow::Cow;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// A single usage entry from any provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub timestamp: DateTime<Utc>,
    pub provider: Cow<'static, str>,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub thinking_tokens: u64,
    pub cost_usd: Option<f64>,
    pub message_id: Option<String>,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
}

impl Record {
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_read_tokens
            + self.cache_creation_tokens
            + self.thinking_tokens
    }

    /// Generate a dedup hash from available identity fields.
    /// Uses message_id + request_id when available, falls back to a
    /// content-based hash (timestamp + provider + model + tokens) so that
    /// sources without message_id (Codex, Cline, Cursor, etc.) still dedup.
    #[must_use]
    pub fn dedup_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        match (&self.message_id, &self.request_id) {
            (Some(msg), Some(req)) => {
                msg.hash(&mut hasher);
                req.hash(&mut hasher);
            }
            (Some(msg), None) => {
                msg.hash(&mut hasher);
                self.model.as_deref().unwrap_or("unknown").hash(&mut hasher);
                self.input_tokens.hash(&mut hasher);
                self.output_tokens.hash(&mut hasher);
            }
            _ => {
                // Content-based dedup for records without message_id
                self.timestamp.hash(&mut hasher);
                self.provider.hash(&mut hasher);
                self.model.as_deref().unwrap_or("unknown").hash(&mut hasher);
                self.input_tokens.hash(&mut hasher);
                self.output_tokens.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Generate a string dedup key for cache storage.
    /// Only used during cache insertion, not for in-memory dedup.
    #[must_use]
    pub fn dedup_key(&self) -> String {
        match (&self.message_id, &self.request_id) {
            (Some(msg), Some(req)) => format!("{msg}\0{req}"),
            (Some(msg), None) => {
                let model = self.model.as_deref().unwrap_or("unknown");
                format!(
                    "{}\0{}\0{}\0{}",
                    msg, model, self.input_tokens, self.output_tokens
                )
            }
            _ => {
                let model = self.model.as_deref().unwrap_or("unknown");
                format!(
                    "{}\0{}\0{}\0{}\0{}",
                    self.timestamp.timestamp_millis(),
                    self.provider,
                    model,
                    self.input_tokens,
                    self.output_tokens
                )
            }
        }
    }
}

/// Aggregated usage for a single model within a time period
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    /// Normalized model name (provider prefixes and date suffixes stripped).
    /// Used for display via `display_model()` and for aggregation keys.
    pub model: String,
    /// First raw model name seen for this aggregation group.
    /// Retains provider prefixes (e.g. `"vertexai.claude-opus-4-6"`) so
    /// `infer_api_provider()` can still detect the routing layer.
    #[serde(default)]
    pub raw_model: String,
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub thinking_tokens: u64,
    pub cost_usd: f64,
    pub request_count: u64,
}

impl ModelUsage {
    /// Sum of all token fields.
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_read_tokens
            + self.cache_creation_tokens
            + self.thinking_tokens
    }

    /// The raw model name to use for API provider inference.
    /// Falls back to the normalized `model` if `raw_model` is empty.
    #[must_use]
    pub fn effective_raw_model(&self) -> &str {
        if self.raw_model.is_empty() {
            &self.model
        } else {
            &self.raw_model
        }
    }
}

/// Summary for a time period (day, week, or month)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: NaiveDate,
    pub label: String,
    pub models: Vec<ModelUsage>,
    pub total_input: u64,
    pub total_output: u64,
    pub total_thinking: u64,
    pub total_cost: f64,
    pub total_requests: u64,
}

impl DailySummary {
    #[must_use]
    pub fn total_cache_creation(&self) -> u64 {
        self.models.iter().map(|m| m.cache_creation_tokens).sum()
    }

    #[must_use]
    pub fn total_cache_read(&self) -> u64 {
        self.models.iter().map(|m| m.cache_read_tokens).sum()
    }

    #[must_use]
    pub fn total_cache(&self) -> u64 {
        self.total_cache_creation() + self.total_cache_read()
    }
}

/// Full report structure (serializable to JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub period: String,
    pub generated_at: DateTime<Utc>,
    pub providers_found: Vec<String>,
    pub summaries: Vec<DailySummary>,
    pub total_cost: f64,
    pub total_tokens: u64,
}

/// Summary for a single coding session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub date: NaiveDate,
    pub client: String,
    pub dominant_model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub thinking_tokens: u64,
    pub total_tokens: u64,
    pub cost: f64,
}

/// Report structure for session output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionReport {
    pub generated_at: DateTime<Utc>,
    pub sessions: Vec<SessionSummary>,
    pub total_cost: f64,
    pub total_tokens: u64,
}
