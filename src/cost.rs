use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::types::Record;

const PRICING_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const CACHE_TTL_SECS: u64 = 3600; // 1 hour

#[derive(Debug, Clone, Deserialize)]
pub struct ModelPricing {
    pub input_cost_per_token: Option<f64>,
    pub output_cost_per_token: Option<f64>,
    #[serde(alias = "cache_read_input_token_cost")]
    pub cache_read_cost: Option<f64>,
    #[serde(alias = "cache_creation_input_token_cost")]
    pub cache_creation_cost: Option<f64>,
}

pub struct PricingEngine {
    models: HashMap<String, ModelPricing>,
}

impl PricingEngine {
    pub fn load(offline: bool) -> Result<Self> {
        let cache_path = Self::cache_path();

        // Check if cache is fresh
        if let Some(data) = Self::read_cache(&cache_path) {
            return Self::parse_pricing(&data);
        }

        if offline {
            if let Some(data) = Self::read_stale_cache(&cache_path) {
                if let Ok(engine) = Self::parse_pricing(&data) {
                    return Ok(engine);
                }
                eprintln!("[tokemon] Warning: cached pricing data corrupt; costs will be $0.00");
            }
            eprintln!("[tokemon] Warning: no cached pricing data and --offline specified; costs will be $0.00");
            return Ok(Self {
                models: HashMap::new(),
            });
        }

        // Fetch from remote
        match Self::fetch_remote() {
            Ok(data) => {
                // Save to cache
                if let Some(parent) = cache_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&cache_path, &data);
                Self::parse_pricing(&data)
            }
            Err(e) => {
                // Fall back to stale cache if available
                if let Some(data) = Self::read_stale_cache(&cache_path) {
                    if let Ok(engine) = Self::parse_pricing(&data) {
                        eprintln!(
                            "[tokemon] Warning: failed to fetch pricing: {e}; using cached prices"
                        );
                        return Ok(engine);
                    }
                }
                eprintln!("[tokemon] Warning: failed to fetch pricing: {e}; costs will be $0.00");
                Ok(Self {
                    models: HashMap::new(),
                })
            }
        }
    }

    /// Returns `true` if the engine has any pricing data loaded.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Apply costs to all entries in-place, caching pricing lookups per model.
    pub fn apply_costs(&self, entries: &mut [Record]) {
        use std::collections::HashMap;
        let mut pricing_cache: HashMap<&str, Option<&ModelPricing>> = HashMap::new();

        for entry in entries.iter_mut() {
            // If entry already has a cost (even $0.00), keep it.
            // Some(0.0) means "already priced, result was zero" (e.g.
            // free model or no pricing data). Re-pricing would cause
            // cost fluctuations when records are loaded from cache.
            if entry.cost_usd.is_some() {
                continue;
            }

            let model = match &entry.model {
                Some(m) if !m.is_empty() => m.as_str(),
                _ => {
                    entry.cost_usd = Some(0.0);
                    continue;
                }
            };

            let pricing = pricing_cache
                .entry(model)
                .or_insert_with(|| self.find_pricing(model));

            let cost = match pricing {
                Some(p) => {
                    let mut c = 0.0;
                    c += entry.input_tokens as f64 * p.input_cost_per_token.unwrap_or(0.0);
                    c += entry.output_tokens as f64 * p.output_cost_per_token.unwrap_or(0.0);
                    c += entry.cache_read_tokens as f64 * p.cache_read_cost.unwrap_or(0.0);
                    c += entry.cache_creation_tokens as f64 * p.cache_creation_cost.unwrap_or(0.0);
                    c += entry.thinking_tokens as f64 * p.output_cost_per_token.unwrap_or(0.0);
                    c
                }
                None => 0.0,
            };
            entry.cost_usd = Some(cost);
        }
    }

    /// Three-level model matching
    fn find_pricing(&self, model: &str) -> Option<&ModelPricing> {
        // Strip source-level provider prefix (e.g., "vertexai." from Vertex AI detection)
        // so that the model name is clean for lookup against litellm pricing data.
        let model = model.strip_prefix("vertexai.").unwrap_or(model);

        // 1. Exact match
        if let Some(p) = self.models.get(model) {
            return Some(p);
        }

        // 2. Normalized match (strip date suffix, lowercase)
        let normalized = normalize_model_name(model);
        if let Some(p) = self.models.get(&normalized) {
            return Some(p);
        }

        // 3. Try with common provider prefixes
        let prefixed_variants = [
            format!("anthropic/{model}"),
            format!("anthropic/{normalized}"),
            format!("openai/{model}"),
            format!("openai/{normalized}"),
            format!("google/{model}"),
            format!("google/{normalized}"),
            format!("vertex_ai/{model}"),
            format!("vertex_ai/{normalized}"),
        ];
        for variant in &prefixed_variants {
            if let Some(p) = self.models.get(variant.as_str()) {
                return Some(p);
            }
        }

        // 4. Prefix match - longest match wins, requires word boundary
        let mut best_match: Option<&ModelPricing> = None;
        let mut best_len: usize = 0;

        for (key, pricing) in &self.models {
            let plain_key = key.split('/').next_back().unwrap_or(key);
            let norm_key = normalize_model_name(plain_key);

            // Only match if our model starts with the pricing key
            // AND the match ends at a word boundary (delimiter or end of string)
            if normalized.starts_with(&norm_key) && norm_key.len() > best_len {
                let at_boundary = normalized.len() == norm_key.len()
                    || matches!(
                        normalized.as_bytes().get(norm_key.len()),
                        Some(b'-' | b'_' | b'.')
                    );
                if at_boundary {
                    best_match = Some(pricing);
                    best_len = norm_key.len();
                }
            }
        }

        best_match
    }

    fn cache_path() -> PathBuf {
        paths::cache_dir().join("pricing.json")
    }

    fn read_cache(path: &Path) -> Option<String> {
        fs::metadata(path)
            .ok()?
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .filter(|age| age.as_secs() <= CACHE_TTL_SECS)
            .and_then(|_| fs::read_to_string(path).ok())
    }

    /// Read cache regardless of age — used as fallback when remote fetch fails.
    fn read_stale_cache(path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    fn fetch_remote() -> Result<String> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| TokemonError::PricingFetch(e.to_string()))?;
        let resp = client
            .get(PRICING_URL)
            .send()
            .map_err(|e| TokemonError::PricingFetch(e.to_string()))?;
        let text = resp
            .text()
            .map_err(|e| TokemonError::PricingFetch(e.to_string()))?;
        Ok(text)
    }

    fn parse_pricing(data: &str) -> Result<Self> {
        let models: HashMap<String, ModelPricing> = serde_json::from_str(data).map_err(|e| {
            TokemonError::PricingFetch(format!("failed to parse pricing JSON: {e}"))
        })?;
        Ok(Self { models })
    }
}

fn normalize_model_name(model: &str) -> String {
    let s = model.to_lowercase();
    let stripped = crate::display::strip_date_suffix(&s);
    stripped.replace('.', "-")
}
