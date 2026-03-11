//! File watcher for live data updates.
//!
//! Uses the `notify` crate to watch source data directories for changes.
//! When a file changes, it re-parses that file and writes the updated
//! records to the `SQLite` cache. The TUI is then notified via the event
//! channel to re-poll the cache.

use std::path::PathBuf;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;

use crate::source::SourceSet;
use crate::{cache, dedup};

use super::event::Event;

/// Start the file watcher in the background.
///
/// This spawns a thread that watches all available source data directories
/// for file changes. When changes are detected (debounced to 500ms), it
/// re-parses the changed files and updates the `SQLite` cache, then sends
/// an [`Event::DataChanged`] through the event channel.
///
/// # Arguments
///
/// * `event_tx` — channel to notify the TUI of data changes and warnings
pub fn start(event_tx: mpsc::UnboundedSender<Event>) {
    std::thread::spawn(move || {
        if let Err(e) = run_watcher(&event_tx) {
            let _ = event_tx.send(Event::Warning(format!("File watcher failed: {e}")));
        }
    });
}

/// Send a warning through the event channel. If the channel is closed,
/// silently discard — the TUI is shutting down.
fn warn(tx: &mpsc::UnboundedSender<Event>, msg: String) {
    let _ = tx.send(Event::Warning(msg));
}

fn run_watcher(event_tx: &mpsc::UnboundedSender<Event>) -> anyhow::Result<()> {
    let registry = SourceSet::new();
    let available = registry.available();

    if available.is_empty() {
        return Ok(());
    }

    // Collect unique data directories to watch
    let mut watch_dirs: Vec<PathBuf> = available
        .iter()
        .map(|s| s.data_dir())
        .filter(|d| d.exists())
        .collect();
    watch_dirs.sort();
    watch_dirs.dedup();

    if watch_dirs.is_empty() {
        return Ok(());
    }

    // Channel for debounced file events
    let (tx, rx) = std::sync::mpsc::channel();

    // Create a debouncer with 500ms delay
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;

    // Watch each data directory recursively
    for dir in &watch_dirs {
        if let Err(e) = debouncer
            .watcher()
            .watch(dir, notify::RecursiveMode::Recursive)
        {
            warn(event_tx, format!("Could not watch {}: {e}", dir.display()));
        }
    }

    // Process debounced events
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Only process if there are actual file changes (not just metadata)
                let has_changes = events
                    .iter()
                    .any(|e| matches!(e.kind, DebouncedEventKind::Any));

                if has_changes {
                    // Re-parse changed files and update the cache
                    if let Err(e) = incremental_update(&registry, event_tx) {
                        warn(event_tx, format!("Incremental update failed: {e}"));
                    }

                    // Notify the TUI
                    if event_tx.send(Event::DataChanged).is_err() {
                        // TUI has been dropped, stop watching
                        break;
                    }
                }
            }
            Ok(Err(errors)) => {
                warn(event_tx, format!("Watch error: {errors}"));
            }
            Err(_) => {
                // Channel closed, debouncer dropped
                break;
            }
        }
    }

    Ok(())
}

/// Re-discover files, parse any that have changed, and update the cache.
///
/// This mirrors the logic in `main.rs::parse_with_cache` but is designed
/// to run from a background thread. It only re-parses files whose
/// modification time has changed since the last cache write.
fn incremental_update(
    registry: &SourceSet,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> anyhow::Result<()> {
    let mut cache = cache::Cache::open()?;
    let cached_mtimes = cache.cached_file_mtimes()?;

    let providers = registry.available();

    // Discover all files and find changed ones.
    // Use WAL-aware mtime for .db files.
    let mut files_to_parse: Vec<(&dyn crate::source::Source, PathBuf, i64)> = Vec::new();
    for provider in &providers {
        for file in provider.discover_files() {
            let mtime = cache::file_mtime_secs_for_db(&file).unwrap_or(0);
            let file_key = file.display().to_string();
            if cached_mtimes.get(&file_key) == Some(&mtime) {
                continue;
            }
            files_to_parse.push((*provider, file, mtime));
        }
    }

    if files_to_parse.is_empty() {
        return Ok(());
    }

    // Parse changed files and collect results
    let mut parsed_files: Vec<(&std::path::Path, i64, Vec<crate::types::Record>)> = Vec::new();

    for (provider, file, mtime) in &files_to_parse {
        match provider.parse_file(file) {
            Ok(entries) => {
                // Don't apply pricing here — the cache stores raw source
                // data. Pricing is applied at read time in
                // load_records_from_cache(), matching the CLI path.
                let entries = dedup::deduplicate(entries);
                parsed_files.push((file.as_path(), *mtime, entries));
            }
            Err(e) => {
                // Log but continue — don't let one bad file block others.
                // This will be reported via Event::Warning by the caller.
                warn(event_tx, format!("failed to parse {}: {e}", file.display()));
            }
        }
    }

    // Write all results in a single transaction
    if !parsed_files.is_empty() {
        cache.write_entries(&parsed_files)?;
    }

    Ok(())
}
