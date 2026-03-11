//! Shared data-loading pipeline.
//!
//! Discovers provider files, parses them (with cache acceleration),
//! de-duplicates, and optionally applies cost pricing.
//! Used by the CLI command handlers and the MCP server.

use chrono::NaiveDate;

use crate::cache::{self, Cache};
use crate::cli::Cli;
use crate::config::Config;
use crate::cost;
use crate::dedup;
use crate::error;
use crate::source::{self, SourceSet};
use crate::types;

const REDISCOVERY_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    pub providers: Vec<String>,
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    pub no_cost: bool,
    pub offline: bool,
    pub refresh: bool,
    pub reparse: bool,
}

impl PipelineOptions {
    #[must_use]
    pub fn from_cli_config(cli: &Cli, config: &Config) -> Self {
        Self {
            providers: if cli.providers.is_empty() {
                config.providers.clone()
            } else {
                cli.providers.clone()
            },
            since: cli.since,
            until: cli.until,
            no_cost: cli.no_cost || config.no_cost,
            offline: cli.offline || config.offline,
            refresh: cli.refresh || config.refresh,
            reparse: cli.reparse || config.reparse,
        }
    }
}

/// Load usage entries from all matching providers, parse via cache, and
/// optionally apply pricing.
pub fn load_and_price(
    opts: &PipelineOptions,
    force_offline: bool,
) -> crate::error::Result<Vec<types::Record>> {
    let registry = SourceSet::new();
    let mut entries = parse_with_cache(&registry, opts)?;

    if !opts.no_cost {
        let offline = force_offline || opts.offline;
        match cost::PricingEngine::load(offline) {
            Ok(engine) => engine.apply_costs(&mut entries),
            Err(e) => {
                if !force_offline {
                    eprintln!("[tokemon] Warning: pricing unavailable: {e}");
                }
            }
        }
    }

    Ok(entries)
}

/// Parse entries using cache. Strategy:
/// 1. Get all cached (file, mtime) pairs in one query
/// 2. Discover provider files and check which have changed
/// 3. Only parse changed files, store results in cache
/// 4. Load everything from cache in one bulk query
fn parse_with_cache(
    registry: &SourceSet,
    opts: &PipelineOptions,
) -> crate::error::Result<Vec<types::Record>> {
    let mut cache = match Cache::open() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("[tokemon] Warning: cache unavailable ({e}); parsing all files");
            None
        }
    };

    let providers = resolve_source_refs(registry, &opts.providers)?;

    let Some(ref mut cache) = cache else {
        return Ok(parse_all_directly(&providers));
    };

    let has_filters = opts.since.is_some() || opts.until.is_some() || !opts.providers.is_empty();

    // If cache is fresh and no --refresh/--reparse flag, skip discovery entirely
    if !opts.refresh && !opts.reparse && !cache.should_rediscover(REDISCOVERY_INTERVAL_SECS) {
        let mut entries = if has_filters {
            cache.load_entries_filtered(opts.since, opts.until, &opts.providers)?
        } else {
            cache.load_all_entries()?
        };
        // Dedup is handled inside load_all_entries / load_entries_filtered.
        entries.sort_by_key(|e| e.timestamp);
        return Ok(entries);
    }

    // When --reparse, ignore cached mtimes so every file gets re-parsed
    let cached_mtimes = if opts.reparse {
        std::collections::HashMap::new()
    } else {
        cache.cached_file_mtimes().unwrap_or_default()
    };

    // Discover all files and collect their paths for preservation tracking
    let all_discovered: Vec<(&dyn source::Source, std::path::PathBuf)> = providers
        .iter()
        .flat_map(|provider| {
            provider
                .discover_files()
                .into_iter()
                .map(move |file| (*provider, file))
        })
        .collect();

    let discovered_files: std::collections::HashSet<String> = all_discovered
        .iter()
        .map(|(_, file)| file.display().to_string())
        .collect();

    // Mark entries from deleted files as preserved (only when discovering all providers,
    // otherwise we'd incorrectly mark entries from non-filtered providers).
    // Best-effort: log a warning if it fails rather than aborting the pipeline.
    if opts.providers.is_empty() {
        if let Err(e) = cache.mark_preserved(&discovered_files) {
            eprintln!("[tokemon] Warning: failed to mark preserved entries: {e}");
        }
    }

    // Find files that need (re)parsing.
    // Use WAL-aware mtime for .db files so we detect SQLite WAL writes.
    let files_to_parse: Vec<_> = all_discovered
        .into_iter()
        .filter_map(|(provider, file)| {
            let mtime = cache::file_mtime_secs_for_db(&file).unwrap_or(0);
            let file_key = file.display().to_string();
            if cached_mtimes.get(&file_key) == Some(&mtime) {
                None
            } else {
                Some((provider, file, mtime))
            }
        })
        .collect();

    // Parse changed files in parallel, then store in a single transaction
    if files_to_parse.is_empty() {
        // No files changed, but update the discovery timestamp so we
        // don't re-discover on the next invocation within the interval.
        if let Err(e) = cache.set_last_discovery() {
            eprintln!("[tokemon] Warning: failed to update discovery timestamp: {e}");
        }
    } else {
        use rayon::prelude::*;

        // Parse in parallel
        let parsed: Vec<_> = files_to_parse
            .par_iter()
            .filter_map(|(provider, file, mtime)| match provider.parse_file(file) {
                Ok(entries) => Some((file.as_path(), *mtime, entries)),
                Err(e) => {
                    eprintln!(
                        "[tokemon] Warning: failed to parse {}: {}",
                        file.display(),
                        e
                    );
                    None
                }
            })
            .collect();

        // Write all parsed results in a single transaction
        if !parsed.is_empty() {
            match cache.write_entries(&parsed) {
                Ok(n) => {
                    if n == 0 {
                        eprintln!("[tokemon] Warning: parsed files but wrote 0 entries to cache");
                    }
                }
                Err(e) => {
                    eprintln!("[tokemon] Warning: cache write failed: {e}");
                    // Fall through — we'll still load whatever is in the cache
                }
            }
        }
    }

    let mut entries = if has_filters {
        cache.load_entries_filtered(opts.since, opts.until, &opts.providers)?
    } else {
        cache.load_all_entries()?
    };
    // Dedup is handled inside load_all_entries / load_entries_filtered.
    entries.sort_by_key(|e| e.timestamp);
    Ok(entries)
}

fn resolve_source_refs<'a>(
    registry: &'a SourceSet,
    filter: &[String],
) -> crate::error::Result<Vec<&'a dyn source::Source>> {
    if filter.is_empty() {
        return Ok(registry.available());
    }

    filter
        .iter()
        .map(|name| {
            registry
                .get(name)
                .ok_or_else(|| error::TokemonError::ProviderNotFound(name.clone()))
        })
        .collect()
}

fn parse_all_directly(providers: &[&dyn source::Source]) -> Vec<types::Record> {
    let mut entries = Vec::new();
    for provider in providers {
        match provider.parse_all() {
            Ok(e) => entries.extend(e),
            Err(e) => eprintln!("[tokemon] Warning: {}: {}", provider.name(), e),
        }
    }
    entries = dedup::deduplicate(entries);
    entries.sort_by_key(|e| e.timestamp);
    entries
}
