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
fn test_continue_parse_fixture() {
    let provider = tokemon::source::continue_dev::ContinueSource::new();
    let path = Path::new("tests/fixtures/continue_tokens.jsonl");
    let entries = provider.parse_file(path).unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].provider, "continue");
    assert_eq!(
        entries[0].model.as_deref(),
        Some("anthropic/claude-sonnet-4")
    );
    assert_eq!(entries[0].input_tokens, 1_234);
    assert_eq!(entries[0].output_tokens, 321);
    assert_eq!(entries[1].model.as_deref(), Some("vertexai.gemini-2.5-pro"));
    assert_eq!(entries[1].input_tokens, 800);
    assert_eq!(entries[1].output_tokens, 200);
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

#[test]
fn test_cli_sandbox_isolation_with_tokemon_home() {
    use std::fs;
    use std::process::Command;

    let sandbox = tempfile::tempdir().expect("create temp sandbox");
    let home = sandbox.path();

    // Seed a Claude Code session inside the isolated TOKEMON_HOME
    let session_dir = home.join(".claude/projects/demo");
    fs::create_dir_all(&session_dir).expect("create session dir");
    fs::copy(
        "tests/fixtures/claude_sample.jsonl",
        session_dir.join("session.jsonl"),
    )
    .expect("copy fixture");

    let bin = env!("CARGO_BIN_EXE_tokemon");

    let init_out = Command::new(bin)
        .arg("init")
        .env("TOKEMON_HOME", home)
        .env_remove("TOKEMON_CACHE_DIR")
        .env_remove("TOKEMON_CONFIG_DIR")
        .output()
        .expect("run tokemon init");
    assert!(
        init_out.status.success(),
        "tokemon init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    assert!(
        home.join(".config/tokemon/config.toml").is_file(),
        "expected isolated config.toml inside TOKEMON_HOME"
    );

    let discover_out = Command::new(bin)
        .arg("discover")
        .env("TOKEMON_HOME", home)
        .env_remove("TOKEMON_CACHE_DIR")
        .env_remove("TOKEMON_CONFIG_DIR")
        .output()
        .expect("run tokemon discover");
    assert!(
        discover_out.status.success(),
        "tokemon discover failed: {}",
        String::from_utf8_lossy(&discover_out.stderr)
    );
    let discover_stdout = String::from_utf8_lossy(&discover_out.stdout);
    assert!(
        discover_stdout.contains("Claude Code") && discover_stdout.contains("Found"),
        "expected Claude Code to be discovered in sandbox, got:\n{discover_stdout}"
    );

    let report_out = Command::new(bin)
        .args(["report", "--offline", "--json"])
        .env("TOKEMON_HOME", home)
        .env_remove("TOKEMON_CACHE_DIR")
        .env_remove("TOKEMON_CONFIG_DIR")
        .output()
        .expect("run tokemon report");
    assert!(
        report_out.status.success(),
        "tokemon report failed: {}",
        String::from_utf8_lossy(&report_out.stderr)
    );
    assert!(
        home.join(".cache/tokemon/usage.db").is_file(),
        "expected isolated usage.db inside TOKEMON_HOME"
    );

    let report: serde_json::Value =
        serde_json::from_slice(&report_out.stdout).expect("parse report JSON");
    assert_eq!(report["total_tokens"].as_u64(), Some(1575));
    assert_eq!(
        report["providers_found"],
        serde_json::json!(["claude-code"])
    );
}

/// Encode a `gen_metadata` row (protobuf) the way Antigravity writes it.
fn antigravity_generation(steps: &[u64], model_id: &str) -> Vec<u8> {
    fn varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }
    fn len_field(number: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = varint(number << 3 | 2);
        out.extend(varint(payload.len() as u64));
        out.extend_from_slice(payload);
        out
    }
    let packed: Vec<u8> = steps.iter().flat_map(|s| varint(*s)).collect();
    let mut blob = len_field(2, &packed);
    blob.extend(len_field(1, &len_field(19, model_id.as_bytes())));
    blob
}

#[test]
fn test_cli_antigravity_model_from_conversation_db_and_cache_upgrade() {
    use std::fs;
    use std::process::Command;

    let sandbox = tempfile::tempdir().expect("create temp sandbox");
    let home = sandbox.path();
    let gemini = home.join(".gemini/antigravity");

    let logs = gemini.join("brain/conv-1/.system_generated/logs");
    fs::create_dir_all(&logs).expect("create transcript dir");
    fs::write(
        logs.join("transcript.jsonl"),
        concat!(
            r#"{"step_index":4,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:05Z","input_tokens":100,"output_tokens":20,"cache_read_tokens":30}"#,
            "\n"
        ),
    )
    .expect("write transcript");

    let bin = env!("CARGO_BIN_EXE_tokemon");
    let report = || -> serde_json::Value {
        let out = Command::new(bin)
            .args(["report", "--offline", "--json", "--no-cost"])
            .env("TOKEMON_HOME", home)
            .env_remove("TOKEMON_CACHE_DIR")
            .env_remove("TOKEMON_CONFIG_DIR")
            .output()
            .expect("run tokemon report");
        assert!(
            out.status.success(),
            "tokemon report failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("parse report JSON")
    };
    let models = |report: &serde_json::Value| -> Vec<String> {
        report["summaries"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["models"].as_array().unwrap().iter())
            .map(|m| m["model"].as_str().unwrap().to_string())
            .collect()
    };

    // No conversation database yet: the transcript is cached without a model.
    let first = report();
    assert_eq!(first["total_tokens"].as_u64(), Some(150));
    assert_eq!(models(&first), vec!["unknown"]);

    // The database appears without the transcript changing. A cache written by
    // an older release (before parser versions existed) must be rebuilt.
    let conversations = gemini.join("conversations");
    fs::create_dir_all(&conversations).expect("create conversations dir");
    let db = rusqlite::Connection::open(conversations.join("conv-1.db")).expect("create db");
    db.execute_batch("CREATE TABLE gen_metadata (idx integer, data blob, PRIMARY KEY (idx));")
        .expect("create table");
    db.execute(
        "INSERT INTO gen_metadata (idx, data) VALUES (0, ?1)",
        rusqlite::params![antigravity_generation(&[4, 5], "gemini-4-hydrogen")],
    )
    .expect("insert generation");
    drop(db);

    let cache =
        rusqlite::Connection::open(home.join(".cache/tokemon/usage.db")).expect("open cache");
    cache
        .execute(
            "DELETE FROM cache_meta WHERE key LIKE 'parser_version:%'",
            [],
        )
        .expect("simulate cache from an older release");
    drop(cache);

    let second = report();
    assert_eq!(
        second["total_tokens"].as_u64(),
        Some(150),
        "tokens must not change"
    );
    assert_eq!(models(&second), vec!["gemini-4-hydrogen"]);
}

#[test]
fn test_cli_date_filtering_with_cache_and_fallback_and_config_defaults() {
    use std::fs;
    use std::process::Command;

    let sandbox = tempfile::tempdir().expect("create temp sandbox");
    let home = sandbox.path();

    // Seed two Claude Code entries on different dates (2026-01-10 and 2026-01-20)
    let session_dir = home.join(".claude/projects/demo");
    fs::create_dir_all(&session_dir).expect("create session dir");
    fs::write(
        session_dir.join("multi.jsonl"),
        concat!(
            r#"{"type":"assistant","timestamp":"2026-01-10T12:00:00Z","message":{"id":"m1","model":"claude-sonnet-4-20250514","usage":{"input_tokens":100,"output_tokens":50}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-20T12:00:00Z","message":{"id":"m2","model":"claude-sonnet-4-20250514","usage":{"input_tokens":200,"output_tokens":100}}}"#,
            "\n"
        ),
    )
    .expect("write fixture");

    let bin = env!("CARGO_BIN_EXE_tokemon");

    // 1. Date filtering with normal SQLite cache (--since and --until)
    let out = Command::new(bin)
        .args([
            "report",
            "--offline",
            "--no-cost",
            "--json",
            "--since",
            "2026-01-15",
            "--until",
            "2026-01-25",
        ])
        .env("TOKEMON_HOME", home)
        .env_remove("TOKEMON_CACHE_DIR")
        .env_remove("TOKEMON_CONFIG_DIR")
        .output()
        .expect("run tokemon report");
    assert!(out.status.success());
    let rep: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse json");
    assert_eq!(rep["total_tokens"].as_u64(), Some(300));

    // 2. Date filtering when cache is unavailable (TOKEMON_CACHE_DIR points to a regular file)
    let bad_cache = home.join("not-a-dir");
    fs::write(&bad_cache, "block cache dir").expect("write blocker file");
    let fallback_out = Command::new(bin)
        .args([
            "report",
            "--offline",
            "--no-cost",
            "--json",
            "--since",
            "2026-01-01",
            "--until",
            "2026-01-15",
        ])
        .env("TOKEMON_HOME", home)
        .env("TOKEMON_CACHE_DIR", &bad_cache)
        .env_remove("TOKEMON_CONFIG_DIR")
        .output()
        .expect("run tokemon report fallback");
    assert!(fallback_out.status.success());
    let fb_rep: serde_json::Value =
        serde_json::from_slice(&fallback_out.stdout).expect("parse fallback json");
    assert_eq!(fb_rep["total_tokens"].as_u64(), Some(150));

    // 3. Config default_command = "weekly" and default_format = "json" work without CLI flags
    let cfg_dir = home.join(".config/tokemon");
    fs::create_dir_all(&cfg_dir).expect("create config dir");
    fs::write(
        cfg_dir.join("config.toml"),
        "default_command = \"weekly\"\ndefault_format = \"json\"\n",
    )
    .expect("write config.toml");

    let cfg_out = Command::new(bin)
        .args(["report", "--offline", "--no-cost"])
        .env("TOKEMON_HOME", home)
        .env_remove("TOKEMON_CACHE_DIR")
        .env_remove("TOKEMON_CONFIG_DIR")
        .output()
        .expect("run tokemon report with config defaults");
    assert!(cfg_out.status.success());
    let cfg_rep: serde_json::Value =
        serde_json::from_slice(&cfg_out.stdout).expect("parse config-default json");
    assert_eq!(cfg_rep["period"].as_str(), Some("weekly"));
    assert_eq!(cfg_rep["total_tokens"].as_u64(), Some(450));
}
