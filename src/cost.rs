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
                match Self::parse_pricing(&data) {
                    Ok(engine) => {
                        // Save to cache only if valid
                        if let Some(parent) = cache_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(&cache_path, &data);
                        Ok(engine)
                    }
                    Err(e) => {
                        // Fall back to stale cache if available
                        if let Some(data) = Self::read_stale_cache(&cache_path) {
                            if let Ok(engine) = Self::parse_pricing(&data) {
                                eprintln!(
                                    "[tokemon] Warning: failed to parse remote pricing: {e}; using cached prices"
                                );
                                return Ok(engine);
                            }
                        }
                        eprintln!("[tokemon] Warning: failed to parse remote pricing: {e}; costs will be $0.00");
                        Ok(Self {
                            models: HashMap::new(),
                        })
                    }
                }
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

    /// Apply costs to all entries in-place, caching pricing lookups per model.
    pub fn apply_costs(&self, entries: &mut [Record]) {
        use std::collections::HashMap;
        let mut pricing_cache: HashMap<&str, Option<&ModelPricing>> = HashMap::new();

        for entry in entries.iter_mut() {
            // Skip records that already have a positive cost — these were
            // priced correctly on a previous run and re-pricing would cause
            // cost fluctuations when records are loaded from cache.
            //
            // Records with `Some(0.0)` are treated as *unpriced*: some
            // source parsers store `cost: 0` when they don't know the
            // price for a model, so we give the pricing engine a chance
            // to fill in the real cost.
            if entry.cost_usd.is_some_and(|c| c > 0.0) {
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

    /// Resolve model pricing, preferring an explicit Vertex route.
    fn find_pricing(&self, model: &str) -> Option<&ModelPricing> {
        let (route, plain_model) = split_pricing_route(model);
        let normalized = normalize_model_name(plain_model);
        let mut candidates = Vec::with_capacity(10);

        if let Some(provider) = route {
            push_model_candidates(&mut candidates, Some(provider), plain_model, &normalized);
        }
        push_model_candidates(&mut candidates, None, plain_model, &normalized);

        for provider in ordered_provider_prefixes(route, &normalized) {
            push_model_candidates(&mut candidates, Some(provider), plain_model, &normalized);
        }

        for candidate in candidates {
            if let Some(pricing) = self.models.get(&candidate) {
                return Some(pricing);
            }
        }

        // Fall back to a deterministic longest-prefix match. Provider rank
        // breaks ties so an explicit route cannot select another provider's
        // otherwise-equivalent entry.
        let mut matches: Vec<(&str, &ModelPricing, usize, usize)> = self
            .models
            .iter()
            .filter_map(|(key, pricing)| {
                let (key_route, plain_key) = split_pricing_route(key);
                let normalized_key = normalize_model_name(plain_key);
                model_prefix_matches(&normalized, &normalized_key).then_some((
                    key.as_str(),
                    pricing,
                    normalized_key.len(),
                    provider_rank(route, &normalized, key_route),
                ))
            })
            .collect();
        matches.sort_unstable_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.3.cmp(&b.3))
                .then_with(|| a.0.cmp(b.0))
        });
        matches.first().map(|(_, pricing, _, _)| *pricing)
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
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(35))
            .timeout_connect(std::time::Duration::from_secs(5))
            .timeout_read(std::time::Duration::from_secs(30))
            .build();
        let resp = agent
            .get(PRICING_URL)
            .call()
            .map_err(|e| TokemonError::Pricing(e.to_string()))?;
        let text = resp
            .into_string()
            .map_err(|e| TokemonError::Pricing(e.to_string()))?;
        Ok(text)
    }

    fn parse_pricing(data: &str) -> Result<Self> {
        let models: HashMap<String, ModelPricing> = serde_json::from_str(data)
            .map_err(|e| TokemonError::Pricing(format!("failed to parse pricing JSON: {e}")))?;
        Ok(Self { models })
    }
}

fn normalize_model_name(model: &str) -> String {
    let s = strip_deployment_suffix(model).to_lowercase();
    let stripped = crate::display::strip_date_suffix(&s);
    stripped.replace('.', "-")
}

const PRICING_PROVIDERS: [&str; 4] = ["anthropic", "openai", "google", "vertex_ai"];

fn split_pricing_route(model: &str) -> (Option<&str>, &str) {
    let route =
        (model.starts_with("vertexai.") || model.starts_with("vertex_ai/")).then_some("vertex_ai");
    (route, crate::display::strip_routing_prefix(model))
}

fn strip_deployment_suffix(model: &str) -> &str {
    model.split('@').next().unwrap_or(model)
}

fn push_model_candidates(
    candidates: &mut Vec<String>,
    provider: Option<&str>,
    model: &str,
    normalized: &str,
) {
    for variant in [model, normalized] {
        let candidate = provider.map_or_else(
            || variant.to_string(),
            |provider| format!("{provider}/{variant}"),
        );
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
}

fn ordered_provider_prefixes(route: Option<&str>, normalized_model: &str) -> Vec<&'static str> {
    let inferred = infer_pricing_provider(normalized_model);
    let mut providers = Vec::with_capacity(PRICING_PROVIDERS.len());

    if let Some(provider) = inferred {
        providers.push(provider);
    }
    for provider in PRICING_PROVIDERS {
        if Some(provider) != route && !providers.contains(&provider) {
            providers.push(provider);
        }
    }

    providers
}

fn infer_pricing_provider(model: &str) -> Option<&'static str> {
    if model.starts_with("claude-") {
        Some("anthropic")
    } else if model.starts_with("gpt-")
        || model.starts_with("o1-")
        || model.starts_with("o3-")
        || model.starts_with("o4-")
    {
        Some("openai")
    } else if model.starts_with("gemini-") || model.starts_with("gemma-") {
        Some("google")
    } else {
        None
    }
}

fn model_prefix_matches(model: &str, prefix: &str) -> bool {
    model.starts_with(prefix)
        && (model.len() == prefix.len()
            || matches!(model.as_bytes().get(prefix.len()), Some(b'-' | b'_' | b'.')))
}

fn provider_rank(route: Option<&str>, model: &str, candidate_route: Option<&str>) -> usize {
    if let Some(expected) = route {
        return match candidate_route {
            Some(actual) if expected == actual => 0,
            None => 1,
            Some(_) => 2,
        };
    }

    match candidate_route {
        Some(actual) if Some(actual) == infer_pricing_provider(model) => 0,
        None => 1,
        Some(_) => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY_JSON: &str = r#"{
        "model-a": {
            "input_cost_per_token": 0.001,
            "output_cost_per_token": 0.002
        },
        "anthropic/claude-3-5-sonnet-20241022": {
            "input_cost_per_token": 0.003,
            "output_cost_per_token": 0.015,
            "cache_read_input_token_cost": 0.0003,
            "cache_creation_input_token_cost": 0.00375
        },
        "gpt-4o-mini": {
            "input_cost_per_token": 0.00015,
            "output_cost_per_token": 0.0006
        }
    }"#;

    const ROUTED_PRICING_JSON: &str = r#"{
        "claude-opus-5": {
            "input_cost_per_token": 0.000005,
            "output_cost_per_token": 0.000025
        },
        "vertex_ai/claude-opus-5": {
            "input_cost_per_token": 0.000007,
            "output_cost_per_token": 0.000035
        }
    }"#;

    #[test]
    fn test_parse_pricing_valid_json() {
        let engine = PricingEngine::parse_pricing(DUMMY_JSON).expect("Failed to parse dummy JSON");
        assert!(!engine.models.is_empty());
        assert_eq!(engine.models.len(), 3);

        let model_a = engine.models.get("model-a").expect("model-a missing");
        assert_eq!(model_a.input_cost_per_token, Some(0.001));
        assert_eq!(model_a.output_cost_per_token, Some(0.002));
        assert_eq!(model_a.cache_read_cost, None);

        let claude = engine
            .models
            .get("anthropic/claude-3-5-sonnet-20241022")
            .expect("claude missing");
        assert_eq!(claude.cache_read_cost, Some(0.0003));
        assert_eq!(claude.cache_creation_cost, Some(0.00375));
    }

    #[test]
    fn test_parse_pricing_invalid_json() {
        let bad_json = r#"{ "model": { "input_cost_per_token": "not-a-number" } }"#;
        let result = PricingEngine::parse_pricing(bad_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_pricing_exact_and_normalized() {
        let engine = PricingEngine::parse_pricing(DUMMY_JSON).unwrap();

        // 1. Exact match
        let p1 = engine.find_pricing("model-a").expect("should find model-a");
        assert_eq!(p1.input_cost_per_token, Some(0.001));

        // 2. Normalized match (strip date suffix)
        // 'gpt-4o-mini-2024-07-18' -> normalizes to 'gpt-4o-mini'
        let p2 = engine
            .find_pricing("gpt-4o-mini-2024-07-18")
            .expect("should normalize to gpt-4o-mini");
        assert_eq!(p2.input_cost_per_token, Some(0.00015));

        // 3. Normalized match replacing dots with dashes
        // 'gpt-4o.mini' -> normalizes to 'gpt-4o-mini'
        let p3 = engine
            .find_pricing("gpt-4o.mini")
            .expect("should normalize dots to dashes");
        assert_eq!(p3.input_cost_per_token, Some(0.00015));
    }

    #[test]
    fn test_find_pricing_prefixes() {
        let engine = PricingEngine::parse_pricing(DUMMY_JSON).unwrap();

        // Exact match with provider in pricing key
        let p1 = engine
            .find_pricing("anthropic/claude-3-5-sonnet-20241022")
            .expect("should find exact");
        assert_eq!(p1.input_cost_per_token, Some(0.003));

        // It should match common provider prefixes added dynamically during find_pricing
        // "claude-3-5-sonnet-20241022" shouldn't match exact because key has "anthropic/"
        // but `find_pricing` will check variants like `anthropic/{model}`.
        let p2 = engine
            .find_pricing("claude-3-5-sonnet-20241022")
            .expect("should find with added provider prefix");
        assert_eq!(p2.input_cost_per_token, Some(0.003));

        // Also test the vertexai. stripping
        let p3 = engine
            .find_pricing("vertexai.claude-3-5-sonnet-20241022")
            .expect("should strip vertexai. prefix");
        assert_eq!(p3.input_cost_per_token, Some(0.003));
    }

    #[test]
    fn test_claude_wrapper_variants_resolve_equivalently() {
        let engine = PricingEngine::parse_pricing(DUMMY_JSON).unwrap();
        let expected = engine
            .find_pricing("claude-3-5-sonnet-20241022")
            .expect("plain model should resolve");

        for model in [
            "anthropic/claude-3-5-sonnet-20241022",
            "vertexai.claude-3-5-sonnet-20241022",
            "bedrock/anthropic.claude-3-5-sonnet-20241022",
            "azure/anthropic.claude-3-5-sonnet-20241022",
            "openai/claude-3-5-sonnet-20241022",
        ] {
            let resolved = engine
                .find_pricing(model)
                .unwrap_or_else(|| panic!("{model} should resolve"));
            assert!(
                std::ptr::eq(resolved, expected),
                "{model} should resolve to the same pricing entry"
            );
        }
    }

    #[test]
    fn test_find_pricing_longest_prefix() {
        let engine = PricingEngine::parse_pricing(
            r#"{
            "gpt-4": { "input_cost_per_token": 0.03 },
            "gpt-4-32k": { "input_cost_per_token": 0.06 }
        }"#,
        )
        .unwrap();

        // "gpt-4-0613" should match "gpt-4" via prefix match because "gpt-4" is a prefix
        let p1 = engine
            .find_pricing("gpt-4-0613")
            .expect("should prefix match gpt-4");
        assert_eq!(p1.input_cost_per_token, Some(0.03));

        // "gpt-4-32k-0613" should match "gpt-4-32k" (longest match wins)
        let p2 = engine
            .find_pricing("gpt-4-32k-0613")
            .expect("should prefix match gpt-4-32k");
        assert_eq!(p2.input_cost_per_token, Some(0.06));
    }

    #[test]
    fn test_vertex_route_prefers_provider_specific_pricing() {
        let engine = PricingEngine::parse_pricing(ROUTED_PRICING_JSON).unwrap();

        let direct = engine
            .find_pricing("claude-opus-5")
            .expect("generic model should resolve");
        assert_eq!(direct.input_cost_per_token, Some(0.000005));

        let routed = engine
            .find_pricing("vertexai.claude-opus-5")
            .expect("routed model should resolve");
        assert_eq!(routed.input_cost_per_token, Some(0.000007));
    }

    #[test]
    fn test_vertex_routed_record_gets_provider_specific_cost() {
        use chrono::Utc;
        use std::borrow::Cow;

        let engine = PricingEngine::parse_pricing(ROUTED_PRICING_JSON).unwrap();
        let mut records = [Record {
            timestamp: Utc::now(),
            provider: Cow::Borrowed("test"),
            model: Some("vertexai.claude-opus-5".to_string()),
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            thinking_tokens: 0,
            cost_usd: Some(0.0),
            message_id: None,
            request_id: None,
            session_id: None,
        }];

        engine.apply_costs(&mut records);

        assert_eq!(records[0].cost_usd, Some(42.0));
    }

    #[test]
    fn test_vertex_route_normalizes_deployment_alias() {
        let engine = PricingEngine::parse_pricing(ROUTED_PRICING_JSON).unwrap();

        let routed = engine
            .find_pricing("vertexai.claude-opus-5@default")
            .expect("deployment alias should resolve");
        assert_eq!(routed.output_cost_per_token, Some(0.000035));
    }

    #[test]
    fn test_vertex_route_deterministically_prefers_provider_fallback() {
        let engine = PricingEngine::parse_pricing(
            r#"{
                "anthropic/claude-opus-5-20260724": {
                    "input_cost_per_token": 0.000005
                },
                "vertex_ai/claude-opus-5@20260724": {
                    "input_cost_per_token": 0.000007
                }
            }"#,
        )
        .unwrap();

        for _ in 0..10 {
            let routed = engine
                .find_pricing("vertexai.claude-opus-5@default")
                .expect("provider fallback should resolve");
            assert_eq!(routed.input_cost_per_token, Some(0.000007));
        }
    }

    #[test]
    fn test_zero_cost_gets_repriced() {
        use chrono::Utc;
        use std::borrow::Cow;

        let engine = PricingEngine::parse_pricing(DUMMY_JSON).unwrap();

        let mut records = vec![
            // Record with cost_usd = Some(0.0) should be re-priced
            Record {
                timestamp: Utc::now(),
                provider: Cow::Borrowed("test"),
                model: Some("model-a".to_string()),
                input_tokens: 1000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                thinking_tokens: 0,
                cost_usd: Some(0.0),
                message_id: None,
                request_id: None,
                session_id: None,
            },
            // Record with a positive cost should be kept as-is
            Record {
                timestamp: Utc::now(),
                provider: Cow::Borrowed("test"),
                model: Some("model-a".to_string()),
                input_tokens: 1000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                thinking_tokens: 0,
                cost_usd: Some(99.0),
                message_id: None,
                request_id: None,
                session_id: None,
            },
            // Record with cost_usd = None should also be priced
            Record {
                timestamp: Utc::now(),
                provider: Cow::Borrowed("test"),
                model: Some("model-a".to_string()),
                input_tokens: 1000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                thinking_tokens: 0,
                cost_usd: None,
                message_id: None,
                request_id: None,
                session_id: None,
            },
        ];

        engine.apply_costs(&mut records);

        // model-a: input=0.001, output=0.002
        // expected = 1000 * 0.001 + 500 * 0.002 = 1.0 + 1.0 = 2.0
        let expected_cost = 2.0;

        // Some(0.0) record got re-priced
        assert_eq!(
            records[0].cost_usd,
            Some(expected_cost),
            "record with cost_usd=Some(0.0) should be re-priced"
        );

        // Positive cost record kept original value
        assert_eq!(
            records[1].cost_usd,
            Some(99.0),
            "record with positive cost should not be re-priced"
        );

        // None record got priced
        assert_eq!(
            records[2].cost_usd,
            Some(expected_cost),
            "record with cost_usd=None should be priced"
        );
    }
}
