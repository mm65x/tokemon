//! Display name translation for clients, models, and API providers.
//!
//! Keeps all presentation logic separate from rendering layout.

use std::borrow::Cow;

/// Map a raw client identifier (source name) to a human-readable display name.
/// "claude-code" -> "Claude Code", "roo-code" -> "Roo Code", etc.
/// Unknown identifiers are returned as-is with title case.
#[must_use]
pub fn display_client(raw: &str) -> Cow<'static, str> {
    match raw {
        "claude-code" => "Claude Code".into(),
        "codex" => "Codex CLI".into(),
        "gemini" => "Gemini CLI".into(),
        "opencode" => "OpenCode".into(),
        "amp" => "Amp".into(),
        "cline" => "Cline".into(),
        "roo-code" => "Roo Code".into(),
        "kilo-code" => "Kilo Code".into(),
        "copilot" => "GitHub Copilot".into(),
        "pi-agent" => "Pi Agent".into(),
        "kimi" => "Kimi".into(),
        "droid" => "Droid".into(),
        "openclaw" => "OpenClaw".into(),
        "qwen" => "Qwen Code".into(),
        "piebald" => "Piebald".into(),
        "cursor" => "Cursor".into(),
        other => Cow::Owned(title_case(other)),
    }
}

/// Normalize a raw model name to a canonical form for aggregation.
///
/// Strips provider routing prefixes (`vertexai.`, `openai/`, `bedrock/`, etc.),
/// `@deployment` suffixes, and date suffixes (`-YYYYMMDD`).
/// Keeps the `claude-` prefix so that model identity is preserved.
///
/// This ensures the same model used via different sources (e.g., Claude Code
/// vs OpenCode via Vertex AI) aggregates into a single row.
///
/// ```text
/// "vertexai.claude-opus-4-6@default" -> "claude-opus-4-6"
/// "claude-opus-4-6"                  -> "claude-opus-4-6"
/// "openai/gpt-4o"                    -> "gpt-4o"
/// "bedrock/anthropic.claude-sonnet-4-20250514" -> "claude-sonnet-4"
/// ```
#[must_use]
pub fn normalize_model(raw: &str) -> String {
    strip_date_suffix(strip_routing_prefix(raw)).to_string()
}

/// Normalize a raw model name for display.
/// Strips provider prefixes (vertexai., openai/, anthropic/, etc.),
/// the `claude-` prefix, and date suffixes (-YYYYMMDD).
///
/// "claude-opus-4-6-20250805" -> "opus-4-6"
/// "vertexai.gemini-2.5-flash" -> "gemini-2.5-flash"
/// "openai/gpt-4o" -> "gpt-4o"
#[must_use]
pub fn display_model(raw: &str) -> String {
    let s = strip_routing_prefix(raw);

    if let Some(rest) = s.strip_prefix("claude-") {
        return strip_date_suffix(rest).to_string();
    }

    strip_date_suffix(s).to_string()
}

/// Strip API routing prefixes from a model name.
///
/// Handles `@deploy` suffixes, slash-based prefixes (`openai/`, `bedrock/`),
/// and dot-based prefixes (`vertexai.`, `anthropic.`).
///
/// Returns a `&str` borrowed from the input — no allocation.
pub fn strip_routing_prefix(raw: &str) -> &str {
    // Strip @... deployment suffix
    let raw = raw.split('@').next().unwrap_or(raw);
    // Strip slash-based prefixes (e.g., "bedrock/", "openai/")
    let after_slash = raw.split('/').next_back().unwrap_or(raw);
    // Strip dot-based prefixes (e.g., "vertexai.", "anthropic.")
    after_slash
        .strip_prefix("vertexai.")
        .or_else(|| after_slash.strip_prefix("anthropic."))
        .unwrap_or(after_slash)
}

/// Infer the API provider from the raw model name.
/// Uses explicit prefixes first, then falls back to model name patterns.
///
/// "vertexai.gemini-2.5-flash" -> "Vertex AI"
/// "claude-opus-4-1" -> "Anthropic"
/// "gpt-4o" -> "OpenAI"
#[must_use]
pub fn infer_api_provider(raw_model: &str) -> &'static str {
    // Explicit provider prefixes
    if raw_model.starts_with("vertexai.") {
        return "Vertex AI";
    }
    if raw_model.starts_with("openai/") {
        return "OpenAI";
    }
    if raw_model.starts_with("anthropic/") {
        return "Anthropic";
    }
    if raw_model.starts_with("google/") {
        return "Google";
    }
    if raw_model.starts_with("bedrock/") || raw_model.starts_with("amazon.") {
        return "AWS Bedrock";
    }
    if raw_model.starts_with("azure/") {
        return "Azure";
    }
    if raw_model.starts_with("mistral/") {
        return "Mistral";
    }

    // Strip any remaining slash-prefix for pattern matching
    let model = raw_model.split('/').next_back().unwrap_or(raw_model);

    // Infer from model name patterns
    if model.starts_with("claude-") {
        return "Anthropic";
    }
    if model.starts_with("gemini-") || model.starts_with("gemma-") {
        return "Google";
    }
    if model.starts_with("gpt-")
        || model_matches_prefix(model, "o1")
        || model_matches_prefix(model, "o3")
        || model_matches_prefix(model, "o4")
    {
        return "OpenAI";
    }
    if model.starts_with("qwen") {
        return "Alibaba";
    }
    if model.starts_with("deepseek") {
        return "DeepSeek";
    }
    if model.starts_with("mistral") || model.starts_with("codestral") {
        return "Mistral";
    }
    if model.contains("llama") {
        return "Meta";
    }

    ""
}

/// Check if model matches a prefix at a word boundary (exact match or followed by `-`).
fn model_matches_prefix(model: &str, prefix: &str) -> bool {
    model == prefix
        || model
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('-'))
}

/// Strip a trailing date suffix like `-YYYYMMDD` from a model name.
///
/// Returns the input unchanged if no date suffix is found.
pub fn strip_date_suffix(s: &str) -> &str {
    if s.chars().count() >= 9 {
        let idx = s.char_indices().rev().nth(8).map_or(0, |(i, _)| i);
        let last_9 = &s[idx..];
        if last_9.starts_with('-')
            && last_9[1..].len() == 8
            && last_9[1..].chars().all(|c| c.is_ascii_digit())
        {
            return &s[..idx];
        }
    }
    s
}

fn title_case(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let mut result = c.to_uppercase().to_string();
                    result.extend(chars);
                    result
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_client() {
        assert_eq!(display_client("claude-code"), "Claude Code");
        assert_eq!(display_client("roo-code"), "Roo Code");
        assert_eq!(display_client("opencode"), "OpenCode");
        assert_eq!(display_client("copilot"), "GitHub Copilot");
        // Unknown gets title-cased
        assert_eq!(display_client("my-tool"), "My Tool");
    }

    #[test]
    fn test_normalize_model() {
        // Same model from different sources should normalize identically
        assert_eq!(
            normalize_model("vertexai.claude-opus-4-6@default"),
            "claude-opus-4-6"
        );
        assert_eq!(normalize_model("claude-opus-4-6"), "claude-opus-4-6");
        assert_eq!(
            normalize_model("claude-opus-4-6@default"),
            "claude-opus-4-6"
        );
        // Date suffixes stripped
        assert_eq!(
            normalize_model("claude-opus-4-1-20250805"),
            "claude-opus-4-1"
        );
        // Non-Claude models
        assert_eq!(normalize_model("openai/gpt-4o"), "gpt-4o");
        assert_eq!(
            normalize_model("vertexai.gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
        // Bedrock double prefix
        assert_eq!(
            normalize_model("bedrock/anthropic.claude-sonnet-4-20250514"),
            "claude-sonnet-4"
        );
        // Models without any prefix pass through
        assert_eq!(
            normalize_model("trinity-large-preview-free"),
            "trinity-large-preview-free"
        );
        assert_eq!(normalize_model("big-pickle"), "big-pickle");
    }

    #[test]
    fn test_display_model() {
        assert_eq!(display_model("claude-opus-4-1-20250805"), "opus-4-1");
        assert_eq!(display_model("claude-sonnet-4-20250514"), "sonnet-4");
        assert_eq!(display_model("claude-opus-4-5-20251101"), "opus-4-5");
        assert_eq!(display_model("gpt-5-codex"), "gpt-5-codex");
        assert_eq!(display_model("gemini-2.5-flash"), "gemini-2.5-flash");
        assert_eq!(
            display_model("vertexai.gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
        assert_eq!(display_model("openai/gpt-4o"), "gpt-4o");
        assert_eq!(
            display_model("anthropic/claude-sonnet-4-20250514"),
            "sonnet-4"
        );
        // Double prefix: bedrock/anthropic.claude-*
        assert_eq!(
            display_model("bedrock/anthropic.claude-3-5-sonnet-20241022"),
            "3-5-sonnet"
        );
        assert_eq!(
            display_model("bedrock/anthropic.claude-opus-4-1-20250805"),
            "opus-4-1"
        );
        // @ suffix stripping (OpenCode deployment notation)
        assert_eq!(display_model("claude-opus-4-6@default"), "opus-4-6");
        assert_eq!(
            display_model("vertexai.claude-opus-4-6@default"),
            "opus-4-6"
        );
    }

    #[test]
    fn test_infer_api_provider() {
        assert_eq!(infer_api_provider("vertexai.gemini-2.5-flash"), "Vertex AI");
        assert_eq!(infer_api_provider("openai/gpt-4o"), "OpenAI");
        assert_eq!(infer_api_provider("anthropic/claude-sonnet-4"), "Anthropic");
        assert_eq!(infer_api_provider("claude-opus-4-1-20250805"), "Anthropic");
        assert_eq!(infer_api_provider("gemini-2.5-flash"), "Google");
        assert_eq!(infer_api_provider("gpt-4o"), "OpenAI");
        assert_eq!(infer_api_provider("o1-mini"), "OpenAI");
        assert_eq!(infer_api_provider("deepseek-v3"), "DeepSeek");
        assert_eq!(infer_api_provider("qwen-2.5-coder"), "Alibaba");
        assert_eq!(infer_api_provider("unknown-model"), "");
        // Vertex AI detection via model prefix (Claude Code msg_vrtx_ detection)
        assert_eq!(infer_api_provider("vertexai.claude-opus-4-6"), "Vertex AI");
    }

    /// When GroupBy::Model merges rows across clients, `raw_model` is set to
    /// the normalized name. Verify that `infer_api_provider` on a normalized
    /// name returns the model's native vendor, not a routing layer.
    #[test]
    fn test_normalized_model_infers_native_provider() {
        // All routing variants of the same Claude model
        let variants = [
            "vertexai.claude-opus-4-6@default",
            "claude-opus-4-6",
            "anthropic/claude-opus-4-6",
            "bedrock/anthropic.claude-opus-4-6",
        ];
        for raw in &variants {
            let norm = normalize_model(raw);
            assert_eq!(norm, "claude-opus-4-6", "normalize_model({raw})");
            assert_eq!(
                infer_api_provider(&norm),
                "Anthropic",
                "infer_api_provider(normalize_model({raw}))"
            );
        }

        // Non-Claude models
        let gpt_variants = ["openai/gpt-4o", "gpt-4o"];
        for raw in &gpt_variants {
            let norm = normalize_model(raw);
            assert_eq!(norm, "gpt-4o", "normalize_model({raw})");
            assert_eq!(
                infer_api_provider(&norm),
                "OpenAI",
                "infer_api_provider(normalize_model({raw}))"
            );
        }

        let gemini_variants = [
            "vertexai.gemini-2.5-flash",
            "google/gemini-2.5-flash",
            "gemini-2.5-flash",
        ];
        for raw in &gemini_variants {
            let norm = normalize_model(raw);
            assert_eq!(norm, "gemini-2.5-flash", "normalize_model({raw})");
            assert_eq!(
                infer_api_provider(&norm),
                "Google",
                "infer_api_provider(normalize_model({raw}))"
            );
        }
    }
}
