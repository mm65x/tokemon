use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDate};
use rusqlite::{params, types::Value, Connection, Row};

use std::borrow::Cow;

use crate::dedup;
use crate::paths;
use crate::types::Record;

const DB_FILENAME: &str = "usage.db";

pub struct Cache {
    conn: Connection,
}

impl Cache {
    pub fn open() -> anyhow::Result<Self> {
        let db_path = Self::db_path();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;

        // Set busy timeout FIRST — before any other operation that could
        // encounter a lock held by another process (e.g. the watcher thread).
        conn.busy_timeout(Duration::from_secs(5))?;

        // Configure PRAGMAs individually so each one is fully processed.
        // PRAGMA journal_mode returns a result row — use pragma_update
        // which handles this correctly in rusqlite.
        conn.pragma_update(None, "journal_mode", "wal")?;
        conn.pragma_update(None, "synchronous", "normal")?;
        conn.pragma_update(None, "cache_size", -10_000_i32)?;
        conn.pragma_update(None, "temp_store", "memory")?;
        conn.pragma_update(None, "mmap_size", 268_435_456_i64)?;

        // Verify WAL mode actually took effect
        let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if mode != "wal" {
            eprintln!(
                "[tokemon] Warning: requested WAL journal mode but got '{mode}'; \
                 writes may be slower"
            );
        }

        let cache = Self { conn };
        cache.init_schema()?;

        // Verify writes actually work — a canary test.
        cache.verify_writable()?;

        Ok(cache)
    }

    fn db_path() -> PathBuf {
        paths::cache_dir().join(DB_FILENAME)
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        // Create tables with individual statements so failures are isolated
        // and provide clear error messages.
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS usage_entries (
                id INTEGER PRIMARY KEY,
                provider TEXT NOT NULL,
                source_file TEXT NOT NULL,
                source_mtime INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                model TEXT,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_creation_tokens INTEGER NOT NULL,
                thinking_tokens INTEGER NOT NULL,
                cost_usd REAL,
                message_id TEXT,
                request_id TEXT,
                session_id TEXT,
                dedup_key TEXT
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS cache_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Indexes — each individually so we get a clear error if one fails.
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_timestamp ON usage_entries(timestamp)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_source_file ON usage_entries(source_file, source_mtime)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_provider_timestamp ON usage_entries(provider, timestamp)",
            [],
        )?;

        // Migration: add preserved column if missing
        let has_preserved: bool = self
            .conn
            .prepare(
                "SELECT COUNT(*) FROM pragma_table_info('usage_entries') WHERE name='preserved'",
            )?
            .query_row([], |row| row.get::<_, i64>(0))
            .unwrap_or(0)
            > 0;
        if !has_preserved {
            self.conn.execute(
                "ALTER TABLE usage_entries ADD COLUMN preserved INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_preserved_timestamp ON usage_entries(preserved, timestamp)",
            [],
        )?;

        Ok(())
    }

    /// Verify the database is actually writable by doing a round-trip
    /// write/read/delete to `cache_meta`.
    fn verify_writable(&self) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO cache_meta (key, value) VALUES ('_write_test', '1')",
            [],
        )?;
        let val: String = self.conn.query_row(
            "SELECT value FROM cache_meta WHERE key = '_write_test'",
            [],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            val == "1",
            "cache write verification failed: read back '{val}'"
        );
        self.conn
            .execute("DELETE FROM cache_meta WHERE key = '_write_test'", [])?;
        Ok(())
    }

    /// Get all cached (file, mtime) pairs in one query for bulk staleness checking.
    pub fn cached_file_mtimes(&self) -> anyhow::Result<std::collections::HashMap<String, i64>> {
        let mut map = std::collections::HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT source_file, MAX(source_mtime) FROM usage_entries GROUP BY source_file",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (file, mtime) = row?;
            map.insert(file, mtime);
        }
        Ok(map)
    }

    const ENTRY_COLUMNS: &str = "provider, timestamp, model, input_tokens, output_tokens, \
        cache_read_tokens, cache_creation_tokens, thinking_tokens, \
        cost_usd, message_id, request_id, session_id";

    /// Load ALL cached entries in one query.
    pub fn load_all_entries(&self) -> anyhow::Result<Vec<Record>> {
        let sql = format!(
            "SELECT {} FROM usage_entries ORDER BY timestamp",
            Self::ENTRY_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut entries = Vec::new();
        let mut skipped = 0u64;
        for row in stmt.query_map([], Self::row_to_entry)? {
            match row {
                Ok(record) => entries.push(record),
                Err(_) => skipped += 1,
            }
        }
        if skipped > 0 {
            eprintln!("[tokemon] Warning: skipped {skipped} cached entries with parse errors");
        }
        Ok(dedup::deduplicate(entries))
    }

    /// Load cached entries with SQL-level filtering by date range and provider.
    pub fn load_entries_filtered(
        &self,
        since: Option<NaiveDate>,
        until: Option<NaiveDate>,
        providers: &[String],
    ) -> anyhow::Result<Vec<Record>> {
        let mut conditions: Vec<String> = Vec::new();
        let mut param_values: Vec<Value> = Vec::new();

        if let Some(s) = since {
            conditions.push("timestamp >= ?".to_string());
            param_values.push(Value::Text(s.to_string()));
        }

        if let Some(u) = until {
            if let Some(next) = u.succ_opt() {
                conditions.push("timestamp < ?".to_string());
                param_values.push(Value::Text(next.to_string()));
            }
        }

        if !providers.is_empty() {
            let placeholders: Vec<&str> = providers.iter().map(|_| "?").collect();
            conditions.push(format!("provider IN ({})", placeholders.join(",")));
            for p in providers {
                param_values.push(Value::Text(p.clone()));
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT {} FROM usage_entries{} ORDER BY timestamp",
            Self::ENTRY_COLUMNS,
            where_clause
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut entries = Vec::new();
        let mut skipped = 0u64;
        for row in stmt.query_map(rusqlite::params_from_iter(param_values), Self::row_to_entry)? {
            match row {
                Ok(record) => entries.push(record),
                Err(_) => skipped += 1,
            }
        }
        if skipped > 0 {
            eprintln!("[tokemon] Warning: skipped {skipped} cached entries with parse errors");
        }
        Ok(dedup::deduplicate(entries))
    }

    /// Remove stale entries for a file and store new ones.
    ///
    /// Used directly by parsers to store single-file results.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn store_file_entries(
        &self,
        path: &Path,
        mtime_secs: i64,
        entries: &[Record],
    ) -> anyhow::Result<()> {
        let path_str = path.display().to_string();

        self.conn.execute(
            "DELETE FROM usage_entries WHERE source_file = ?1",
            params![path_str],
        )?;

        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO usage_entries (
                provider, source_file, source_mtime, timestamp, model,
                input_tokens, output_tokens, cache_read_tokens,
                cache_creation_tokens, thinking_tokens, cost_usd,
                message_id, request_id, session_id, dedup_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )?;

        for entry in entries {
            stmt.execute(params![
                &*entry.provider,
                path_str,
                mtime_secs,
                entry.timestamp.to_rfc3339(),
                entry.model,
                entry.input_tokens,
                entry.output_tokens,
                entry.cache_read_tokens,
                entry.cache_creation_tokens,
                entry.thinking_tokens,
                entry.cost_usd,
                entry.message_id,
                entry.request_id,
                entry.session_id,
                Some(entry.dedup_key()),
            ])?;
        }

        Ok(())
    }

    /// Write entries for multiple files in a single transaction.
    ///
    /// Uses rusqlite's `Transaction` API which guarantees commit-or-rollback
    /// semantics. On success, also updates the discovery timestamp.
    ///
    /// Returns the total number of entries written.
    pub fn write_entries(&mut self, files: &[(&Path, i64, Vec<Record>)]) -> anyhow::Result<usize> {
        if files.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut total = 0;

        for (path, mtime, entries) in files {
            let path_str = path.display().to_string();

            tx.execute(
                "DELETE FROM usage_entries WHERE source_file = ?1",
                params![path_str],
            )?;

            let mut stmt = tx.prepare_cached(
                "INSERT INTO usage_entries (
                    provider, source_file, source_mtime, timestamp, model,
                    input_tokens, output_tokens, cache_read_tokens,
                    cache_creation_tokens, thinking_tokens, cost_usd,
                    message_id, request_id, session_id, dedup_key
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;

            for entry in entries {
                stmt.execute(params![
                    &*entry.provider,
                    path_str,
                    *mtime,
                    entry.timestamp.to_rfc3339(),
                    entry.model,
                    entry.input_tokens,
                    entry.output_tokens,
                    entry.cache_read_tokens,
                    entry.cache_creation_tokens,
                    entry.thinking_tokens,
                    entry.cost_usd,
                    entry.message_id,
                    entry.request_id,
                    entry.session_id,
                    Some(entry.dedup_key()),
                ])?;
                total += 1;
            }
        }

        // Update discovery timestamp inside the same transaction
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        tx.execute(
            "INSERT OR REPLACE INTO cache_meta (key, value) VALUES ('last_discovery_at', ?1)",
            params![now.to_string()],
        )?;

        tx.commit()?;
        Ok(total)
    }

    /// Check whether file discovery should be skipped because
    /// the cache was populated recently (within `max_age_secs`).
    #[must_use]
    pub fn should_rediscover(&self, max_age_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let last: Option<u64> = self
            .conn
            .query_row(
                "SELECT value FROM cache_meta WHERE key = 'last_discovery_at'",
                [],
                |row| {
                    let v: String = row.get(0)?;
                    Ok(v.parse::<u64>().unwrap_or(0))
                },
            )
            .ok();

        match last {
            Some(ts) => now.saturating_sub(ts) > max_age_secs,
            None => true,
        }
    }

    /// Record the current time as the last discovery timestamp.
    pub fn set_last_discovery(&self) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.conn.execute(
            "INSERT OR REPLACE INTO cache_meta (key, value) VALUES ('last_discovery_at', ?1)",
            params![now.to_string()],
        )?;
        Ok(())
    }

    /// Mark entries as preserved when their source files no longer exist on disk.
    /// `discovered_files` is the set of currently-existing source file paths.
    pub fn mark_preserved(
        &self,
        discovered_files: &std::collections::HashSet<String>,
    ) -> anyhow::Result<()> {
        if discovered_files.is_empty() {
            return Ok(());
        }

        // Get all distinct source files in the cache
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT source_file FROM usage_entries WHERE preserved = 0")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        let cached_files: Vec<String> = rows.flatten().collect();
        for file in &cached_files {
            if !discovered_files.contains(file) {
                self.conn.execute(
                    "UPDATE usage_entries SET preserved = 1 WHERE source_file = ?1 AND preserved = 0",
                    params![file],
                )?;
            }
        }
        Ok(())
    }

    /// Delete preserved entries with timestamps before the given date.
    pub fn prune_before(&self, before: NaiveDate) -> anyhow::Result<usize> {
        let before_str = before.to_string();
        let deleted = self.conn.execute(
            "DELETE FROM usage_entries WHERE preserved = 1 AND timestamp < ?1",
            params![before_str],
        )?;
        Ok(deleted)
    }

    /// Single source of truth for mapping a SQLite row to a Record.
    /// Column order must match ENTRY_COLUMNS.
    fn row_to_entry(row: &Row) -> rusqlite::Result<Record> {
        let ts_str: String = row.get(1)?;
        let timestamp = DateTime::parse_from_rfc3339(&ts_str)
            .map(|dt| dt.to_utc())
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;

        let provider: String = row.get(0)?;
        Ok(Record {
            timestamp,
            provider: Cow::Owned(provider),
            model: row.get(2)?,
            input_tokens: row.get::<_, i64>(3)?.max(0) as u64,
            output_tokens: row.get::<_, i64>(4)?.max(0) as u64,
            cache_read_tokens: row.get::<_, i64>(5)?.max(0) as u64,
            cache_creation_tokens: row.get::<_, i64>(6)?.max(0) as u64,
            thinking_tokens: row.get::<_, i64>(7)?.max(0) as u64,
            cost_usd: row.get(8)?,
            message_id: row.get(9)?,
            request_id: row.get(10)?,
            session_id: row.get(11)?,
        })
    }
}

/// Get file modification time as seconds since epoch.
pub fn file_mtime_secs(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// Get file modification time, with SQLite WAL awareness.
///
/// For `.db` files, returns the maximum mtime across the main file
/// and its `-wal` and `-shm` siblings. This is necessary because
/// SQLite in WAL mode writes to the `-wal` file first; the main
/// `.db` file's mtime may not update until a checkpoint occurs.
pub fn file_mtime_secs_for_db(path: &Path) -> Option<i64> {
    let base = file_mtime_secs(path)?;

    // Only check WAL/SHM siblings for .db files
    let ext = path.extension().and_then(|e| e.to_str());
    if ext != Some("db") {
        return Some(base);
    }

    let path_str = path.to_string_lossy();
    let wal_mtime = file_mtime_secs(Path::new(&format!("{path_str}-wal"))).unwrap_or(0);
    let shm_mtime = file_mtime_secs(Path::new(&format!("{path_str}-shm"))).unwrap_or(0);

    Some(base.max(wal_mtime).max(shm_mtime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Create an in-memory cache for testing.
    fn test_cache() -> Cache {
        let conn = Connection::open_in_memory().unwrap();
        conn.busy_timeout(Duration::from_secs(5)).unwrap();
        conn.pragma_update(None, "journal_mode", "wal")
            .unwrap_or_else(|_| {
                // In-memory databases don't support WAL — that's expected
            });
        let cache = Cache { conn };
        cache.init_schema().unwrap();
        cache
    }

    fn make_record(provider: &str, timestamp: &str, session_id: Option<&str>) -> Record {
        Record {
            timestamp: DateTime::parse_from_rfc3339(timestamp).unwrap().to_utc(),
            provider: Cow::Owned(provider.to_string()),
            model: Some("test-model".to_string()),
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            thinking_tokens: 0,
            cost_usd: Some(0.01),
            message_id: None,
            request_id: None,
            session_id: session_id.map(String::from),
        }
    }

    #[test]
    fn test_store_and_load() {
        let cache = test_cache();
        let entries = vec![
            make_record("claude-code", "2026-02-20T10:00:00Z", Some("sess-1")),
            make_record("claude-code", "2026-02-21T10:00:00Z", Some("sess-1")),
        ];

        cache
            .store_file_entries(Path::new("/tmp/test.jsonl"), 1000, &entries)
            .unwrap();

        let loaded = cache.load_all_entries().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].provider, "claude-code");
        assert_eq!(loaded[0].session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn test_write_entries_transaction() {
        let mut cache = test_cache();
        let entries_a = vec![make_record("opencode", "2026-03-10T10:00:00Z", None)];
        let entries_b = vec![
            make_record("claude-code", "2026-03-10T11:00:00Z", Some("s1")),
            make_record("claude-code", "2026-03-10T12:00:00Z", Some("s1")),
        ];

        let files: Vec<(&Path, i64, Vec<Record>)> = vec![
            (Path::new("/data/opencode.db"), 5000, entries_a),
            (Path::new("/data/session.jsonl"), 6000, entries_b),
        ];

        let written = cache.write_entries(&files).unwrap();
        assert_eq!(written, 3);

        let loaded = cache.load_all_entries().unwrap();
        assert_eq!(loaded.len(), 3);

        // Verify discovery timestamp was set
        assert!(!cache.should_rediscover(60));
    }

    #[test]
    fn test_mark_preserved() {
        let cache = test_cache();

        // Store entries from two different files
        let entries_a = vec![make_record("claude-code", "2026-02-20T10:00:00Z", None)];
        let entries_b = vec![make_record("codex", "2026-02-21T10:00:00Z", None)];

        cache
            .store_file_entries(Path::new("/data/file_a.jsonl"), 1000, &entries_a)
            .unwrap();
        cache
            .store_file_entries(Path::new("/data/file_b.jsonl"), 2000, &entries_b)
            .unwrap();

        // Only file_a still exists on disk
        let discovered: HashSet<String> = ["/data/file_a.jsonl".to_string()].into_iter().collect();

        cache.mark_preserved(&discovered).unwrap();

        // file_b's entries should be preserved=1
        let preserved_count: i64 = cache
            .conn
            .query_row(
                "SELECT COUNT(*) FROM usage_entries WHERE preserved = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved_count, 1);

        // file_a's entries should still be preserved=0
        let active_count: i64 = cache
            .conn
            .query_row(
                "SELECT COUNT(*) FROM usage_entries WHERE preserved = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_count, 1);

        // All entries still load (preserved entries are regular rows)
        let all = cache.load_all_entries().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_mark_preserved_empty_discovered_is_noop() {
        let cache = test_cache();
        let entries = vec![make_record("claude-code", "2026-02-20T10:00:00Z", None)];
        cache
            .store_file_entries(Path::new("/data/file.jsonl"), 1000, &entries)
            .unwrap();

        // Empty discovered set should not mark anything
        cache.mark_preserved(&HashSet::new()).unwrap();

        let preserved_count: i64 = cache
            .conn
            .query_row(
                "SELECT COUNT(*) FROM usage_entries WHERE preserved = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved_count, 0);
    }

    #[test]
    fn test_prune_before() {
        let cache = test_cache();

        let entries = vec![
            make_record("claude-code", "2025-06-15T10:00:00Z", None),
            make_record("claude-code", "2026-02-20T10:00:00Z", None),
        ];
        cache
            .store_file_entries(Path::new("/data/old.jsonl"), 1000, &entries)
            .unwrap();

        // Mark all as preserved (simulating file deletion)
        // Empty discovered set would be a no-op due to guard, so mark manually
        cache
            .conn
            .execute("UPDATE usage_entries SET preserved = 1", [])
            .unwrap();

        // Prune entries before 2026-01-01
        let deleted = cache
            .prune_before(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
            .unwrap();
        assert_eq!(deleted, 1);

        // Only the 2026 entry should remain
        let remaining = cache.load_all_entries().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            remaining[0].timestamp.date_naive().to_string(),
            "2026-02-20"
        );
    }

    #[test]
    fn test_prune_ignores_non_preserved() {
        let cache = test_cache();

        let entries = vec![make_record("claude-code", "2025-06-15T10:00:00Z", None)];
        cache
            .store_file_entries(Path::new("/data/active.jsonl"), 1000, &entries)
            .unwrap();

        // preserved=0 (default), so prune should not delete it
        let deleted = cache
            .prune_before(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
            .unwrap();
        assert_eq!(deleted, 0);

        let remaining = cache.load_all_entries().unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_preserved_column_migration_idempotent() {
        // Calling init_schema twice should not error
        let cache = test_cache();
        cache.init_schema().unwrap();
        cache.init_schema().unwrap();
    }
}
