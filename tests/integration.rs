use std::path::Path;
use tokemon::source::Source;
use tokemon::types::Record;

#[test]
fn test_claude_code_parse_fixture() {
    let provider = tokemon::source::claude_code::ClaudeCodeSource::new();
    let path = Path::new("tests/fixtures/claude_sample.jsonl");
    let entries = provider.parse_file(path).unwrap();

    // Should have 3 assistant entries (last one is a duplicate of req_003/msg_003)
    // But parse_file doesn't dedup - that happens in parse_all
    assert_eq!(entries.len(), 4);

    // First entry
    assert_eq!(entries[0].provider, "claude-code");
    assert_eq!(
        entries[0].model.as_deref(),
        Some("claude-opus-4-1-20250805")
    );
    assert_eq!(entries[0].input_tokens, 100);
    assert_eq!(entries[0].output_tokens, 50);
    assert_eq!(entries[0].cache_creation_tokens, 500);
    assert_eq!(entries[0].cache_read_tokens, 0);
    assert_eq!(entries[0].request_id.as_deref(), Some("req_001"));
    assert_eq!(entries[0].message_id.as_deref(), Some("msg_001"));

    // Second entry
    assert_eq!(entries[1].input_tokens, 200);
    assert_eq!(entries[1].output_tokens, 150);
    assert_eq!(entries[1].cache_read_tokens, 400);

    // Third entry - different model, different day
    assert_eq!(
        entries[2].model.as_deref(),
        Some("claude-sonnet-4-20250514")
    );
    assert_eq!(entries[2].input_tokens, 50);
}

#[test]
fn test_claude_code_dedup() {
    let provider = tokemon::source::claude_code::ClaudeCodeSource::new();
    let path = Path::new("tests/fixtures/claude_sample.jsonl");
    let entries = provider.parse_file(path).unwrap();

    // Before dedup: 4 entries (duplicate msg_003:req_003)
    assert_eq!(entries.len(), 4);

    let deduped = tokemon::dedup::deduplicate(entries);
    // After dedup: 3 entries (duplicate removed)
    assert_eq!(deduped.len(), 3);
}

#[test]
fn test_codex_parse_fixture() {
    let provider = tokemon::source::codex::CodexSource::new();
    let path = Path::new("tests/fixtures/codex_sample.jsonl");
    let entries = provider.parse_file(path).unwrap();

    assert_eq!(entries.len(), 2);

    // First token_count: input=300, cached=50, so actual_input=250
    assert_eq!(entries[0].provider, "codex");
    assert_eq!(entries[0].model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(entries[0].input_tokens, 250); // 300 - 50 cached
    assert_eq!(entries[0].output_tokens, 100);
    assert_eq!(entries[0].cache_read_tokens, 50);

    // Second: input=500, cached=100, actual=400
    assert_eq!(entries[1].input_tokens, 400);
    assert_eq!(entries[1].output_tokens, 200);
    assert_eq!(entries[1].cache_read_tokens, 100);
}

#[test]
fn test_gemini_parse_fixture() {
    let provider = tokemon::source::gemini::GeminiSource::new();
    let path = Path::new("tests/fixtures/gemini_sample.json");
    let entries = provider.parse_file(path).unwrap();

    assert_eq!(entries.len(), 2);

    assert_eq!(entries[0].provider, "gemini");
    assert_eq!(entries[0].model.as_deref(), Some("gemini-2.5-flash"));
    assert_eq!(entries[0].input_tokens, 150);
    assert_eq!(entries[0].output_tokens, 75);
    assert_eq!(entries[0].cache_read_tokens, 30);
    assert_eq!(entries[0].thinking_tokens, 20);

    assert_eq!(entries[1].input_tokens, 200);
    assert_eq!(entries[1].thinking_tokens, 50);
}

#[test]
fn test_cline_parse_fixture() {
    let provider = tokemon::source::cline::ClineSource::new();
    let path = Path::new("tests/fixtures/cline_sample.json");
    let entries = provider.parse_file(path).unwrap();

    assert_eq!(entries.len(), 2);

    assert_eq!(entries[0].provider, "cline");
    assert_eq!(entries[0].input_tokens, 500);
    assert_eq!(entries[0].output_tokens, 200);
    assert_eq!(entries[0].cache_creation_tokens, 100);
    assert_eq!(entries[0].cache_read_tokens, 50);
    assert_eq!(entries[0].cost_usd, Some(0.015));

    assert_eq!(entries[1].input_tokens, 800);
    assert_eq!(entries[1].cost_usd, Some(0.025));
}

#[test]
fn test_daily_aggregation() {
    let provider = tokemon::source::claude_code::ClaudeCodeSource::new();
    let path = Path::new("tests/fixtures/claude_sample.jsonl");
    let entries = provider.parse_file(path).unwrap();
    let entries = tokemon::dedup::deduplicate(entries);

    let summaries = tokemon::rollup::aggregate_daily(&entries);

    // Should have 2 days: 2026-02-20 and 2026-02-21
    assert_eq!(summaries.len(), 2);

    // First day: 2 entries (opus model)
    assert_eq!(summaries[0].total_requests, 2);
    assert_eq!(summaries[0].total_input, 300); // 100 + 200

    // Second day: 1 entry (sonnet model)
    assert_eq!(summaries[1].total_requests, 1);
    assert_eq!(summaries[1].total_input, 50);
}

#[test]
fn test_date_filtering() {
    use chrono::NaiveDate;

    let provider = tokemon::source::claude_code::ClaudeCodeSource::new();
    let path = Path::new("tests/fixtures/claude_sample.jsonl");
    let entries = provider.parse_file(path).unwrap();

    let since = NaiveDate::from_ymd_opt(2026, 2, 21);
    let filtered = tokemon::rollup::filter_by_date(entries, since, None);

    // Only entries from Feb 21 should remain
    assert_eq!(filtered.len(), 2); // 2 entries on that day (including duplicate)
    for entry in &filtered {
        assert_eq!(entry.timestamp.date_naive().to_string(), "2026-02-21");
    }
}

#[test]
fn test_usage_entry_total_tokens() {
    use chrono::Utc;
    use tokemon::types::Record;

    let entry = Record {
        timestamp: Utc::now(),
        provider: "test".to_string().into(),
        model: None,
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: 30,
        cache_creation_tokens: 20,
        thinking_tokens: 10,
        cost_usd: None,
        message_id: None,
        request_id: None,
        session_id: None,
    };

    assert_eq!(entry.total_tokens(), 210);
}

#[test]
fn test_dedup_key_generation() {
    use chrono::Utc;
    use tokemon::types::Record;

    let entry_both = Record {
        timestamp: Utc::now(),
        provider: "test".to_string().into(),
        model: Some("model-a".to_string()),
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        thinking_tokens: 0,
        cost_usd: None,
        message_id: Some("msg_1".to_string()),
        request_id: Some("req_1".to_string()),
        session_id: None,
    };
    assert_eq!(entry_both.dedup_key(), "msg_1\0req_1".to_string());

    let entry_msg_only = Record {
        message_id: Some("msg_2".to_string()),
        request_id: None,
        ..entry_both.clone()
    };
    assert_eq!(
        entry_msg_only.dedup_key(),
        "msg_2\0model-a\0100\050".to_string()
    );

    let entry_none = Record {
        message_id: None,
        request_id: None,
        ..entry_both.clone()
    };
    // Content-based dedup key includes timestamp, provider, model, tokens
    let key = entry_none.dedup_key();
    assert!(key.contains("test"));
    assert!(key.contains("model-a"));
    assert!(key.contains("100"));
    assert!(key.contains("50"));
}

// --- Session aggregation tests ---

fn make_record(
    provider: &str,
    model: &str,
    timestamp: &str,
    input: u64,
    output: u64,
    cost: f64,
    session_id: Option<&str>,
) -> Record {
    Record {
        timestamp: chrono::DateTime::parse_from_rfc3339(timestamp)
            .unwrap()
            .to_utc(),
        provider: provider.to_string().into(),
        model: Some(model.to_string()),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        thinking_tokens: 0,
        cost_usd: Some(cost),
        message_id: None,
        request_id: None,
        session_id: session_id.map(String::from),
    }
}

#[test]
fn test_aggregate_by_session_basic() {
    let entries = vec![
        make_record(
            "claude-code",
            "claude-opus-4-1-20250805",
            "2026-02-20T10:00:00Z",
            100,
            50,
            1.0,
            Some("sess-aaa"),
        ),
        make_record(
            "claude-code",
            "claude-opus-4-1-20250805",
            "2026-02-20T11:00:00Z",
            200,
            100,
            2.0,
            Some("sess-aaa"),
        ),
        make_record(
            "claude-code",
            "claude-sonnet-4-20250514",
            "2026-02-21T09:00:00Z",
            50,
            25,
            0.5,
            Some("sess-bbb"),
        ),
    ];

    let sessions = tokemon::rollup::aggregate_by_session(&entries);

    assert_eq!(sessions.len(), 2);

    // Sorted by cost descending: sess-aaa ($3.0) before sess-bbb ($0.5)
    assert_eq!(sessions[0].session_id, "sess-aaa");
    assert_eq!(sessions[0].cost, 3.0);
    assert_eq!(sessions[0].input_tokens, 300);
    assert_eq!(sessions[0].output_tokens, 150);
    assert_eq!(sessions[0].total_tokens, 450);
    assert_eq!(sessions[0].client, "Claude Code");
    assert_eq!(sessions[0].dominant_model, "opus-4-1");
    assert_eq!(sessions[0].date.to_string(), "2026-02-20");

    assert_eq!(sessions[1].session_id, "sess-bbb");
    assert_eq!(sessions[1].cost, 0.5);
    assert_eq!(sessions[1].dominant_model, "sonnet-4");
}

#[test]
fn test_aggregate_by_session_skips_no_session_id() {
    let entries = vec![
        make_record(
            "claude-code",
            "claude-opus-4-1",
            "2026-02-20T10:00:00Z",
            100,
            50,
            1.0,
            Some("sess-aaa"),
        ),
        make_record(
            "claude-code",
            "claude-opus-4-1",
            "2026-02-20T11:00:00Z",
            200,
            100,
            2.0,
            None,
        ),
    ];

    let sessions = tokemon::rollup::aggregate_by_session(&entries);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "sess-aaa");
}

#[test]
fn test_aggregate_by_session_dominant_model() {
    // Session with two models: opus has more tokens
    let entries = vec![
        make_record(
            "claude-code",
            "claude-opus-4-1-20250805",
            "2026-02-20T10:00:00Z",
            1000,
            500,
            5.0,
            Some("sess-mixed"),
        ),
        make_record(
            "claude-code",
            "claude-sonnet-4-20250514",
            "2026-02-20T11:00:00Z",
            100,
            50,
            0.5,
            Some("sess-mixed"),
        ),
    ];

    let sessions = tokemon::rollup::aggregate_by_session(&entries);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].dominant_model, "opus-4-1");
}

#[test]
fn test_aggregate_by_session_empty() {
    let entries: Vec<Record> = vec![];
    let sessions = tokemon::rollup::aggregate_by_session(&entries);
    assert!(sessions.is_empty());
}

#[test]
fn test_aggregate_by_session_date_is_earliest() {
    let entries = vec![
        make_record(
            "claude-code",
            "claude-opus-4-1",
            "2026-02-22T15:00:00Z",
            100,
            50,
            1.0,
            Some("sess-x"),
        ),
        make_record(
            "claude-code",
            "claude-opus-4-1",
            "2026-02-20T09:00:00Z",
            200,
            100,
            2.0,
            Some("sess-x"),
        ),
    ];

    let sessions = tokemon::rollup::aggregate_by_session(&entries);
    assert_eq!(sessions[0].date.to_string(), "2026-02-20");
}

#[test]
fn test_session_from_fixture() {
    let provider = tokemon::source::claude_code::ClaudeCodeSource::new();
    let path = Path::new("tests/fixtures/claude_sample.jsonl");
    let entries = provider.parse_file(path).unwrap();
    let entries = tokemon::dedup::deduplicate(entries);

    // All entries get session_id = "claude_sample" from file stem
    assert!(entries
        .iter()
        .all(|e| e.session_id.as_deref() == Some("claude_sample")));

    let sessions = tokemon::rollup::aggregate_by_session(&entries);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "claude_sample");
    assert_eq!(sessions[0].total_tokens, 650 + 750 + 175); // three entries' totals
}
