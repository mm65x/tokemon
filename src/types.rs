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

    /// Add all numeric fields from `other` into `self`.
    /// Used to aggregate multiple `ModelUsage` entries into one.
    pub fn accumulate(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.thinking_tokens += other.thinking_tokens;
        self.cost_usd += other.cost_usd;
        self.request_count += other.request_count;
    }
}

/// Summary for a time period (day, week, or month)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodSummary {
    pub date: NaiveDate,
    pub label: String,
    pub models: Vec<ModelUsage>,
    pub total_input: u64,
    pub total_output: u64,
    pub total_thinking: u64,
    pub total_cost: f64,
    pub total_requests: u64,
}

impl PeriodSummary {
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
    pub summaries: Vec<PeriodSummary>,
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

// ── Group-by mode ─────────────────────────────────────────────────────────

/// How to group rows in the detail table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// One row per model (aggregated across all clients).
    Model,
    /// One row per model+client combination.
    ModelClient,
    /// One row per client (aggregated across all models).
    Client,
}

impl GroupBy {
    /// Cycle to the next group-by mode.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Model => Self::ModelClient,
            Self::ModelClient => Self::Client,
            Self::Client => Self::Model,
        }
    }

    /// Short label for display.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::ModelClient => "model+client",
            Self::Client => "client",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_record_total_tokens() {
        let record = Record {
            timestamp: Utc::now(),
            provider: "test".into(),
            model: None,
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 5,
            cache_creation_tokens: 15,
            thinking_tokens: 50,
            cost_usd: None,
            message_id: None,
            request_id: None,
            session_id: None,
        };
        assert_eq!(record.total_tokens(), 100);
    }

    #[test]
    fn test_record_dedup_hash() {
        let mut r1 = Record {
            timestamp: Utc.timestamp_opt(1000, 0).unwrap(),
            provider: "test".into(),
            model: Some("m1".into()),
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            thinking_tokens: 0,
            cost_usd: None,
            message_id: Some("msg1".into()),
            request_id: Some("req1".into()),
            session_id: None,
        };

        let mut r2 = r1.clone();
        assert_eq!(r1.dedup_hash(), r2.dedup_hash());

        // Different request ID should change hash when both are present
        r2.request_id = Some("req2".into());
        assert_ne!(r1.dedup_hash(), r2.dedup_hash());

        // Message ID only
        r1.request_id = None;
        r2.request_id = None;
        assert_eq!(r1.dedup_hash(), r2.dedup_hash());

        // No IDs
        r1.message_id = None;
        r2.message_id = None;
        assert_eq!(r1.dedup_hash(), r2.dedup_hash());

        // Changing content without IDs changes hash
        r2.input_tokens = 11;
        assert_ne!(r1.dedup_hash(), r2.dedup_hash());
    }

    #[test]
    fn test_model_usage_accumulate() {
        let mut u1 = ModelUsage {
            model: "m1".into(),
            raw_model: "vertexai.m1".into(),
            provider: "vertexai".into(),
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 5,
            cache_creation_tokens: 2,
            thinking_tokens: 1,
            cost_usd: 1.0,
            request_count: 1,
        };

        let u2 = ModelUsage {
            model: "m1".into(),
            raw_model: "".into(),
            provider: "vertexai".into(),
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 50,
            cache_creation_tokens: 20,
            thinking_tokens: 10,
            cost_usd: 10.0,
            request_count: 5,
        };

        u1.accumulate(&u2);

        assert_eq!(u1.input_tokens, 110);
        assert_eq!(u1.output_tokens, 220);
        assert_eq!(u1.cache_read_tokens, 55);
        assert_eq!(u1.cache_creation_tokens, 22);
        assert_eq!(u1.thinking_tokens, 11);
        assert_eq!(u1.total_tokens(), 418);
        assert_eq!(u1.cost_usd, 11.0);
        assert_eq!(u1.request_count, 6);
    }

    #[test]
    fn test_model_usage_effective_raw_model() {
        let u1 = ModelUsage {
            model: "m1".into(),
            raw_model: "vertexai.m1".into(),
            ..Default::default()
        };
        assert_eq!(u1.effective_raw_model(), "vertexai.m1");

        let u2 = ModelUsage {
            model: "m1".into(),
            raw_model: "".into(),
            ..Default::default()
        };
        assert_eq!(u2.effective_raw_model(), "m1");
    }

    #[test]
    fn test_group_by_enum() {
        assert_eq!(GroupBy::Model.next(), GroupBy::ModelClient);
        assert_eq!(GroupBy::ModelClient.next(), GroupBy::Client);
        assert_eq!(GroupBy::Client.next(), GroupBy::Model);

        assert_eq!(GroupBy::Model.label(), "model");
        assert_eq!(GroupBy::ModelClient.label(), "model+client");
        assert_eq!(GroupBy::Client.label(), "client");
    }
}
