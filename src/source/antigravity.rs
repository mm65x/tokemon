use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use crate::error::{Result, TokemonError};
use crate::paths;
use crate::timestamp;
use crate::types::Record;

/// Reads Antigravity usage.
///
/// Token counts come from each conversation's `transcript.jsonl`, which does
/// not record which model produced a step. The model is looked up in the
/// conversation's SQLite database (`conversations/<id>.db`, next to the
/// `brain` directory), whose `gen_metadata` rows list the transcript steps they
/// produced together with the model ID. If that database is missing or cannot
/// be decoded, records are still emitted, just without a model.
pub struct AntigravitySource {
    base_dirs: Vec<PathBuf>,
}

impl Default for AntigravitySource {
    fn default() -> Self {
        Self::new()
    }
}

impl AntigravitySource {
    pub fn new() -> Self {
        let gemini = paths::home_dir().join(".gemini");
        Self {
            base_dirs: vec![
                gemini.join("antigravity").join("brain"),
                gemini.join("antigravity-cli").join("brain"),
            ],
        }
    }
}

/// Minimal deserialization struct for transcript JSONL lines.
/// Only model-response steps (`PLANNER_RESPONSE`) carry token counts.
#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    step_index: Option<u64>,
    created_at: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
}

/// Path of the conversation database for a transcript at
/// `<brain>/<id>/.system_generated/logs/transcript.jsonl`, which lives at
/// `<brain>/../conversations/<id>.db`.
fn conversation_db_path(transcript: &Path) -> Option<PathBuf> {
    let conversation = transcript.ancestors().nth(3)?;
    let id = conversation.file_name()?;
    let brain = conversation.parent()?;
    let mut db_name = id.to_os_string();
    db_name.push(".db");
    Some(brain.parent()?.join("conversations").join(db_name))
}

// ── Protobuf decoding ─────────────────────────────────────────────────────
//
// The database stores generation metadata as protobuf without a published
// schema, so only the few fields needed here are read, using a small
// wire-format reader. Anything unexpected ends decoding of that message and
// results in no model rather than an error.

/// Field 19 of the generation message: the model ID (e.g. `gemini-4-hydrogen`).
const GEN_MODEL_ID: u32 = 19;
/// Field 21 of the generation message: the model's display name.
const GEN_MODEL_DISPLAY: u32 = 21;
/// Field 20 of the generation message: repeated string key/value labels.
const GEN_LABEL: u32 = 20;
/// The label key holding the internal model enum.
const LABEL_MODEL_ENUM: &str = "model_enum";

enum Wire<'a> {
    Varint(u64),
    Len(&'a [u8]),
    /// Fixed-width value; its contents are never needed.
    Fixed,
}

/// Iterator over the `(field number, value)` pairs of one protobuf message.
/// Ends at the first malformed or truncated field.
struct Fields<'a> {
    buf: &'a [u8],
}

fn read_varint(buf: &[u8]) -> Option<(u64, &[u8])> {
    let mut value = 0u64;
    for (i, byte) in buf.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, &buf[i + 1..]));
        }
    }
    None
}

impl<'a> Iterator for Fields<'a> {
    type Item = (u32, Wire<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, rest) = read_varint(self.buf)?;
        let number = u32::try_from(key >> 3).ok()?;
        let (value, rest) = match key & 7 {
            0 => {
                let (v, rest) = read_varint(rest)?;
                (Wire::Varint(v), rest)
            }
            1 => (Wire::Fixed, rest.get(8..)?),
            2 => {
                let (len, rest) = read_varint(rest)?;
                let len = usize::try_from(len).ok()?;
                (Wire::Len(rest.get(..len)?), &rest[len..])
            }
            5 => (Wire::Fixed, rest.get(4..)?),
            _ => return None,
        };
        self.buf = rest;
        Some((number, value))
    }
}

fn fields(buf: &[u8]) -> Fields<'_> {
    Fields { buf }
}

fn text(bytes: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(bytes).ok()?.trim();
    (!s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)).then(|| s.to_string())
}

/// Decode one `gen_metadata` row into the transcript steps it covers and the
/// model that produced them.
fn decode_generation(blob: &[u8]) -> Option<(Vec<u64>, String)> {
    let mut steps = Vec::new();
    let mut meta: Option<&[u8]> = None;
    for (number, value) in fields(blob) {
        match (number, value) {
            // Step indices, normally packed but tolerate the unpacked form.
            (2, Wire::Len(mut packed)) => {
                while let Some((step, rest)) = read_varint(packed) {
                    steps.push(step);
                    packed = rest;
                }
            }
            (2, Wire::Varint(step)) => steps.push(step),
            (1, Wire::Len(inner)) => meta = Some(inner),
            _ => {}
        }
    }

    let (mut id, mut display, mut model_enum) = (None, None, None);
    for (number, value) in fields(meta?) {
        match (number, value) {
            (GEN_MODEL_ID, Wire::Len(b)) => id = text(b),
            (GEN_MODEL_DISPLAY, Wire::Len(b)) => display = text(b),
            (GEN_LABEL, Wire::Len(label)) => {
                let (mut key, mut val) = (None, None);
                for (n, v) in fields(label) {
                    match (n, v) {
                        (1, Wire::Len(b)) => key = text(b),
                        (2, Wire::Len(b)) => val = text(b),
                        _ => {}
                    }
                }
                if key.as_deref() == Some(LABEL_MODEL_ENUM) {
                    model_enum = val;
                }
            }
            _ => {}
        }
    }

    // Prefer the model ID; fall back to the display name, then the raw enum.
    let model = id.or(display).or(model_enum)?;
    (!steps.is_empty()).then_some((steps, model))
}

/// Percent-encode a path into a `file:` URI accepted by SQLite, with `query`
/// (without the leading `?`) appended.
fn sqlite_file_uri(path: &Path, query: &str) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file://");
    if !raw.starts_with('/') {
        uri.push('/'); // Windows drive paths: file:///C:/...
    }
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-' | b'~') {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.push('?');
    uri.push_str(query);
    uri
}

/// Open a conversation database read-only without creating files next to it.
///
/// Normally the database is opened `immutable`, which needs no lock or journal
/// files. A conversation that is being written has a non-empty `-wal` file
/// with data not yet in the main file, so it is opened normally to include
/// that data.
fn open_conversation_db(db: &Path) -> Option<Connection> {
    let mut wal = db.as_os_str().to_os_string();
    wal.push("-wal");
    let has_wal = fs::metadata(&wal).is_ok_and(|m| m.len() > 0);

    let conn = if has_wal {
        Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
    } else {
        Connection::open_with_flags(
            sqlite_file_uri(db, "immutable=1"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
    }
    .ok()?;
    let _ = conn.busy_timeout(Duration::from_secs(2));
    Some(conn)
}

/// Map each transcript step index to the model that produced it.
fn load_step_models(db: &Path) -> HashMap<u64, String> {
    let mut models = HashMap::new();
    if !db.is_file() {
        return models;
    }
    let Some(conn) = open_conversation_db(db) else {
        return models;
    };
    let Ok(mut stmt) = conn.prepare("SELECT data FROM gen_metadata") else {
        return models;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)) else {
        return models;
    };
    for blob in rows.filter_map(std::result::Result::ok) {
        if let Some((steps, model)) = decode_generation(&blob) {
            for step in steps {
                models.entry(step).or_insert_with(|| model.clone());
            }
        }
    }
    models
}

impl super::Source for AntigravitySource {
    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn display_name(&self) -> &'static str {
        "Antigravity"
    }

    fn data_dir(&self) -> PathBuf {
        // Report the primary data directory for the `discover` command.
        self.base_dirs.first().cloned().unwrap_or_default()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for base in &self.base_dirs {
            let Ok(brains) = fs::read_dir(base) else {
                continue;
            };
            for entry in brains.filter_map(std::result::Result::ok) {
                if !entry.path().is_dir() {
                    continue;
                }
                let transcript = entry
                    .path()
                    .join(".system_generated")
                    .join("logs")
                    .join("transcript.jsonl");
                if transcript.is_file() {
                    files.push(transcript);
                }
            }
        }
        files
    }

    /// 1: token counts only. 2: adds the model, read from the conversation
    /// database.
    fn parser_version(&self) -> u32 {
        2
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        let file = fs::File::open(path).map_err(TokemonError::Io)?;
        let reader = BufReader::with_capacity(64 * 1024, file);

        // Extract session ID from the brain UUID in the path.
        // Layout: .../brain/{uuid}/.system_generated/logs/transcript.jsonl
        let session_id = path
            .ancestors()
            .nth(3)
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned());

        let models = conversation_db_path(path)
            .map(|db| load_step_models(&db))
            .unwrap_or_default();

        let entries = reader
            .lines()
            .map_while(std::result::Result::ok)
            .filter(|line| line.contains("\"PLANNER_RESPONSE\""))
            .filter_map(|line| serde_json::from_str::<TranscriptLine>(&line).ok())
            .filter(|parsed| matches!(parsed.line_type.as_deref(), Some("PLANNER_RESPONSE")))
            .filter_map(|parsed| {
                // Require at least one token count to exist.
                let input = parsed.input_tokens?;
                let timestamp = parsed
                    .created_at
                    .as_deref()
                    .and_then(timestamp::parse_timestamp)?;
                let model = parsed
                    .step_index
                    .and_then(|step| models.get(&step))
                    .cloned();

                Some(Record {
                    timestamp,
                    provider: Cow::Borrowed("antigravity"),
                    model,
                    input_tokens: input,
                    output_tokens: parsed.output_tokens.unwrap_or(0),
                    cache_read_tokens: parsed.cache_read_tokens.unwrap_or(0),
                    cache_creation_tokens: 0,
                    thinking_tokens: 0,
                    cost_usd: None,
                    message_id: None,
                    request_id: None,
                    session_id: session_id.clone(),
                })
            })
            .collect();

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;

    fn write_transcript(root: &Path, id: &str, body: &str) -> PathBuf {
        let logs = root.join(id).join(".system_generated").join("logs");
        fs::create_dir_all(&logs).unwrap();
        let file = logs.join("transcript.jsonl");
        fs::write(&file, body).unwrap();
        file
    }

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

    fn len_field(number: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = varint(u64::from(number) << 3 | 2);
        out.extend(varint(payload.len() as u64));
        out.extend_from_slice(payload);
        out
    }

    fn label(key: &str, value: &str) -> Vec<u8> {
        let mut inner = len_field(1, key.as_bytes());
        inner.extend(len_field(2, value.as_bytes()));
        len_field(GEN_LABEL, &inner)
    }

    /// Encode a `gen_metadata` row like the ones Antigravity writes.
    fn generation(
        steps: &[u64],
        id: Option<&str>,
        display: Option<&str>,
        e: Option<&str>,
    ) -> Vec<u8> {
        let mut meta = Vec::new();
        // Token usage submessage (field 4), present in real rows and ignored.
        let mut usage = varint(2 << 3);
        usage.extend(varint(4248));
        meta.extend(len_field(4, &usage));
        if let Some(id) = id {
            meta.extend(len_field(GEN_MODEL_ID, id.as_bytes()));
        }
        if let Some(e) = e {
            meta.extend(label("model_enum", e));
        }
        meta.extend(label("trajectory_id", "abc"));
        if let Some(display) = display {
            meta.extend(len_field(GEN_MODEL_DISPLAY, display.as_bytes()));
        }

        let packed: Vec<u8> = steps.iter().flat_map(|s| varint(*s)).collect();
        let mut blob = len_field(2, &packed);
        blob.extend(len_field(4, b"cascade"));
        blob.extend(len_field(1, &meta));
        blob
    }

    fn write_db(path: &Path, rows: &[Vec<u8>]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE gen_metadata (idx integer, data blob, size integer NOT NULL DEFAULT 0, PRIMARY KEY (idx));",
        )
        .unwrap();
        for (idx, blob) in rows.iter().enumerate() {
            conn.execute(
                "INSERT INTO gen_metadata (idx, data) VALUES (?1, ?2)",
                rusqlite::params![idx as i64, blob],
            )
            .unwrap();
        }
    }

    #[test]
    fn parses_planner_responses_only() {
        let root = std::env::temp_dir().join("tokemon_antigravity_parse");
        let _ = fs::remove_dir_all(&root);
        let body = concat!(
            r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","created_at":"2026-01-01T00:00:00Z","content":"hi"}"#,
            "\n",
            r#"{"step_index":2,"source":"MODEL","type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:05Z","input_tokens":100,"cache_read_tokens":40,"output_tokens":7}"#,
            "\n",
            "not json PLANNER_RESPONSE\n",
            r#"{"step_index":3,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:09Z"}"#,
            "\n",
            r#"{"step_index":5,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:01:00Z","input_tokens":5,"output_tokens":1}"#,
            "\n",
        );
        let file = write_transcript(&root, "abc-123", body);

        let source = AntigravitySource {
            base_dirs: vec![root.clone()],
        };
        let records = source.parse_file(&file).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].cache_read_tokens, 40);
        assert_eq!(records[0].output_tokens, 7);
        assert_eq!(records[0].total_tokens(), 147);
        assert_eq!(records[0].session_id.as_deref(), Some("abc-123"));
        assert_eq!(records[1].cache_read_tokens, 0);
        assert!(records.iter().all(|r| r.provider == "antigravity"));
        // No conversation database next to this transcript: no model.
        assert!(records.iter().all(|r| r.model.is_none()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discovers_transcripts_across_base_dirs() {
        let a = std::env::temp_dir().join("tokemon_antigravity_disc_a");
        let b = std::env::temp_dir().join("tokemon_antigravity_disc_b");
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
        write_transcript(&a, "one", "");
        write_transcript(&b, "two", "");
        fs::create_dir_all(a.join("empty-brain")).unwrap();

        let source = AntigravitySource {
            base_dirs: vec![a.clone(), b.clone(), PathBuf::from("/nonexistent/path")],
        };
        assert_eq!(source.discover_files().len(), 2);

        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn attaches_model_from_conversation_database() {
        let root = std::env::temp_dir().join("tokemon_antigravity_models");
        let _ = fs::remove_dir_all(&root);
        let brain = root.join("brain");
        let body = concat!(
            r#"{"step_index":6,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:05Z","input_tokens":10,"output_tokens":2,"cache_read_tokens":3}"#,
            "\n",
            r#"{"step_index":12,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:09Z","input_tokens":20,"output_tokens":4}"#,
            "\n",
            r#"{"step_index":30,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:12Z","input_tokens":30,"output_tokens":6}"#,
            "\n",
            r#"{"step_index":40,"type":"PLANNER_RESPONSE","created_at":"2026-01-01T00:00:15Z","input_tokens":40,"output_tokens":8}"#,
            "\n",
        );
        let transcript = write_transcript(&brain, "conv-1", body);
        write_db(
            &root.join("conversations").join("conv-1.db"),
            &[
                generation(
                    &[6, 7, 8, 9, 10, 11],
                    Some("gemini-4-hydrogen"),
                    Some("Gemini 4 Hydrogen (High)"),
                    Some("MODEL_PLACEHOLDER_M525"),
                ),
                generation(&[12, 13], Some("claude-sonnet-5-5-medium"), None, None),
                // No model ID: falls back to the display name.
                generation(
                    &[30],
                    None,
                    Some("Gemini 4 Hydrogen (High)"),
                    Some("MODEL_PLACEHOLDER_M525"),
                ),
            ],
        );

        let source = AntigravitySource {
            base_dirs: vec![brain],
        };
        let records = source.parse_file(&transcript).unwrap();

        assert_eq!(records.len(), 4);
        assert_eq!(records[0].model.as_deref(), Some("gemini-4-hydrogen"));
        assert_eq!(
            records[1].model.as_deref(),
            Some("claude-sonnet-5-5-medium")
        );
        assert_eq!(
            records[2].model.as_deref(),
            Some("Gemini 4 Hydrogen (High)")
        );
        // A step the database knows nothing about stays without a model.
        assert_eq!(records[3].model, None);
        // Token counts still come from the transcript.
        assert_eq!(records[0].input_tokens, 10);
        assert_eq!(records[0].cache_read_tokens, 3);
        assert_eq!(records[1].output_tokens, 4);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_live_database_with_wal_and_leaves_no_files_otherwise() {
        let root = std::env::temp_dir().join("tokemon_antigravity_wal");
        let _ = fs::remove_dir_all(&root);
        let db = root.join("conversations").join("c.db");
        write_db(
            &db,
            &[generation(&[1], Some("gemini-4-hydrogen"), None, None)],
        );

        // Idle database: read without creating -wal/-shm files beside it.
        let entries = |dir: &Path| fs::read_dir(dir).unwrap().count();
        let before = entries(db.parent().unwrap());
        assert_eq!(
            load_step_models(&db).get(&1).map(String::as_str),
            Some("gemini-4-hydrogen")
        );
        assert_eq!(entries(db.parent().unwrap()), before);

        // Live database: a writer holds a WAL with data not yet checkpointed.
        let writer = Connection::open(&db).unwrap();
        writer.pragma_update(None, "journal_mode", "wal").unwrap();
        writer
            .execute(
                "INSERT INTO gen_metadata (idx, data) VALUES (99, ?1)",
                rusqlite::params![generation(&[2], Some("claude-opus-5-5-high"), None, None)],
            )
            .unwrap();
        let models = load_step_models(&db);
        assert_eq!(
            models.get(&2).map(String::as_str),
            Some("claude-opus-5-5-high")
        );
        drop(writer);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_or_corrupt_database_yields_no_models() {
        let root = std::env::temp_dir().join("tokemon_antigravity_bad_db");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        assert!(load_step_models(&root.join("absent.db")).is_empty());

        let corrupt = root.join("corrupt.db");
        fs::write(&corrupt, b"definitely not sqlite").unwrap();
        assert!(load_step_models(&corrupt).is_empty());

        // A database without the expected table.
        let other = root.join("other.db");
        Connection::open(&other)
            .unwrap()
            .execute_batch("CREATE TABLE unrelated (x integer);")
            .unwrap();
        assert!(load_step_models(&other).is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn decoder_handles_unpacked_steps_and_rejects_garbage() {
        // Unpacked repeated step field.
        let mut meta = len_field(GEN_MODEL_ID, b"gemini-4-hydrogen");
        meta.extend(label("model_enum", "MODEL_PLACEHOLDER_M1"));
        let mut blob = Vec::new();
        for step in [3u64, 4] {
            blob.extend(varint(2 << 3));
            blob.extend(varint(step));
        }
        blob.extend(len_field(1, &meta));
        let (steps, model) = decode_generation(&blob).unwrap();
        assert_eq!(steps, vec![3, 4]);
        assert_eq!(model, "gemini-4-hydrogen");

        // Falls back to the enum when nothing else names the model.
        let only_enum = generation(&[1], None, None, Some("MODEL_PLACEHOLDER_M7"));
        assert_eq!(
            decode_generation(&only_enum).unwrap().1,
            "MODEL_PLACEHOLDER_M7"
        );

        // Truncated, empty, and non-protobuf input never panics.
        let good = generation(&[1, 2], Some("gemini-4-hydrogen"), None, None);
        for cut in 0..good.len() {
            let _ = decode_generation(&good[..cut]);
        }
        assert!(decode_generation(&[]).is_none());
        assert!(decode_generation(&[0xFF; 32]).is_none());

        // No steps, or a model ID with control characters, is not usable.
        assert!(decode_generation(&len_field(1, &len_field(GEN_MODEL_ID, b"m"))).is_none());
        assert!(decode_generation(&generation(&[1], Some("bad\u{1}id"), None, None)).is_none());
    }

    #[test]
    fn locates_conversation_database_beside_brain() {
        let transcript =
            Path::new("/h/.gemini/antigravity/brain/abc/.system_generated/logs/transcript.jsonl");
        assert_eq!(
            conversation_db_path(transcript),
            Some(PathBuf::from("/h/.gemini/antigravity/conversations/abc.db"))
        );
    }

    #[test]
    fn builds_escaped_sqlite_uris() {
        let uri = sqlite_file_uri(Path::new("/Users/me/My Dir/a#b%c.db"), "immutable=1");
        assert_eq!(uri, "file:///Users/me/My%20Dir/a%23b%25c.db?immutable=1");
        let win = sqlite_file_uri(Path::new(r"C:\Users\me\x.db"), "immutable=1");
        assert_eq!(win, "file:///C%3A/Users/me/x.db?immutable=1");
    }

    #[test]
    fn parser_version_marks_model_aware_parser() {
        assert_eq!(AntigravitySource::new().parser_version(), 2);
    }
}
