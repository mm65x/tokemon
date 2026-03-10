// Pedantic lint suppressions — see lib.rs for rationale.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::doc_markdown
)]

use chrono::{NaiveDate, Utc};
use clap::Parser;

mod cache;
mod cli;
mod config;
mod cost;
mod dedup;
mod display;
mod error;
mod mcp;
mod pacemaker;
mod paths;
mod render;
mod rollup;
mod source;
mod timestamp;
mod tui;
mod types;

use cache::Cache;
use cli::{Cli, Commands, Frequency};
use config::Config;
use source::SourceSet;
use types::{Report, SessionReport};

const REDISCOVERY_INTERVAL_SECS: u64 = 30;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match cli.command.as_ref() {
        None => cmd_report(&cli, &config),
        Some(Commands::Discover) => {
            cmd_discover();
            Ok(())
        }
        Some(Commands::Init) => cmd_init(),
        Some(Commands::Statusline) => cmd_statusline(&cli, &config),
        Some(Commands::Budget) => cmd_budget(&cli, &config),
        Some(Commands::Sessions { top }) => cmd_sessions(&cli, &config, *top),
        Some(Commands::Prune { before }) => cmd_prune(*before),
        Some(Commands::Mcp) => mcp::run(&cli, &config),
        Some(Commands::Top { view, interval }) => {
            // CLI --interval overrides config, config overrides hardcoded default
            let tick = if *interval > 0 {
                *interval
            } else {
                config.tick_interval
            };
            tui::run(&config, view, tick)
        }
    }
}

fn cmd_discover() {
    let registry = SourceSet::new();

    let info: Vec<(&str, &str, bool, String, usize)> = registry
        .all()
        .iter()
        .map(|p| {
            let available = p.is_available();
            let data_dir = p.data_dir().display().to_string();
            let file_count = if available {
                p.discover_files().len()
            } else {
                0
            };
            (p.name(), p.display_name(), available, data_dir, file_count)
        })
        .collect();

    render::print_discover(&info);
}

fn cmd_init() -> anyhow::Result<()> {
    let path = Config::write_default()?;
    println!("Config written to: {}", path.display());
    println!("Edit this file to customize default behavior.");
    Ok(())
}

// --- Shared helpers for command handlers ---

fn resolve_providers<'a>(cli: &'a Cli, config: &'a Config) -> &'a [String] {
    if cli.providers.is_empty() {
        &config.providers
    } else {
        &cli.providers
    }
}

/// Compute the start date for a given frequency.
#[must_use]
fn frequency_since(freq: Frequency) -> NaiveDate {
    match freq {
        Frequency::Daily => timestamp::start_of_today(),
        Frequency::Weekly => timestamp::start_of_week(),
        Frequency::Monthly => timestamp::start_of_month(),
    }
}

/// Merge two optional dates, taking the later of the two.
#[must_use]
fn merge_since(a: Option<NaiveDate>, b: Option<NaiveDate>) -> Option<NaiveDate> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

pub(crate) fn load_and_price(
    cli: &Cli,
    config: &Config,
    force_offline: bool,
    since: Option<NaiveDate>,
    until: Option<NaiveDate>,
) -> anyhow::Result<Vec<types::Record>> {
    let registry = SourceSet::new();
    let filter = resolve_providers(cli, config);
    let force_refresh = cli.refresh || config.refresh;
    let force_reparse = cli.reparse || config.reparse;
    let mut entries = parse_with_cache(
        &registry,
        filter,
        force_refresh,
        force_reparse,
        since,
        until,
    )?;

    if !(cli.no_cost || config.no_cost) {
        let offline = force_offline || cli.offline || config.offline;
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

// --- Command handlers ---

fn cmd_report(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let freq = cli.frequency;
    let period = match freq {
        Frequency::Daily => "daily",
        Frequency::Weekly => "weekly",
        Frequency::Monthly => "monthly",
    };
    let entries = load_and_price(cli, config, false, cli.since, cli.until)?;

    if entries.is_empty() {
        let empty_report = Report {
            period: period.to_string(),
            generated_at: Utc::now(),
            providers_found: Vec::new(),
            summaries: Vec::new(),
            total_cost: 0.0,
            total_tokens: 0,
        };
        if cli.json {
            render::print_json(&empty_report);
        } else if cli.csv {
            let breakdown = cli.display_mode(config) == cli::DisplayMode::Breakdown;
            if breakdown {
                render::print_csv_breakdown(&empty_report);
            } else {
                render::print_csv_compact(&empty_report);
            }
        } else {
            println!("No usage data found.");
            if resolve_providers(cli, config).is_empty() {
                println!("Run `tokemon discover` to see which providers are available.");
            }
        }
        return Ok(());
    }

    let providers_found: Vec<String> = entries
        .iter()
        .map(|e| e.provider.to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut summaries = match freq {
        Frequency::Weekly => rollup::aggregate_weekly(&entries),
        Frequency::Monthly => rollup::aggregate_monthly(&entries),
        Frequency::Daily => rollup::aggregate_daily(&entries),
    };

    if cli.is_desc(config) {
        summaries.reverse();
    }

    let total_cost: f64 = summaries.iter().map(|s| s.total_cost).sum();
    let total_tokens: u64 = entries.iter().map(types::Record::total_tokens).sum();

    let report = Report {
        period: period.to_string(),
        generated_at: Utc::now(),
        providers_found,
        summaries,
        total_cost,
        total_tokens,
    };

    if cli.json {
        render::print_json(&report);
    } else if cli.csv {
        let breakdown = cli.display_mode(config) == cli::DisplayMode::Breakdown;
        if breakdown {
            render::print_csv_breakdown(&report);
        } else {
            render::print_csv_compact(&report);
        }
    } else {
        let breakdown = cli.display_mode(config) == cli::DisplayMode::Breakdown;
        render::print_table(&report, breakdown, &config.columns);
    }

    Ok(())
}

fn cmd_statusline(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let freq = cli.frequency;
    let since = frequency_since(freq);
    let effective_since = merge_since(cli.since, Some(since));
    let entries = load_and_price(cli, config, true, effective_since, cli.until)?;
    let period_label = match freq {
        Frequency::Daily => "today",
        Frequency::Weekly => "this week",
        Frequency::Monthly => "this month",
    };

    let mut providers_seen = std::collections::HashSet::new();
    let (total_cost, total_tokens) = entries
        .iter()
        .filter(|e| e.timestamp.date_naive() >= since)
        .fold((0.0f64, 0u64), |(cost, tokens), e| {
            providers_seen.insert(&*e.provider);
            (cost + e.cost_usd.unwrap_or(0.0), tokens + e.total_tokens())
        });
    let provider_count = providers_seen.len();

    // Append budget info if configured
    let budget_str = if config.budget.daily.is_some()
        || config.budget.weekly.is_some()
        || config.budget.monthly.is_some()
    {
        let (daily, weekly, monthly) = pacemaker::evaluate(&entries, &config.budget);
        match freq {
            Frequency::Daily => daily.map(|(s, l)| format_budget_short(s, l)),
            Frequency::Weekly => weekly.map(|(s, l)| format_budget_short(s, l)),
            Frequency::Monthly => monthly.map(|(s, l)| format_budget_short(s, l)),
        }
    } else {
        None
    };

    match budget_str {
        Some(bs) => println!(
            "${:.2} | {} | {} | {} | {}",
            total_cost,
            render::format_tokens_short(total_tokens),
            format_provider_count(provider_count),
            period_label,
            bs
        ),
        None => render::print_statusline(total_cost, total_tokens, provider_count, period_label),
    }

    Ok(())
}

fn cmd_budget(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let entries = load_and_price(cli, config, false, cli.since, cli.until)?;
    let (daily, weekly, monthly) = pacemaker::evaluate(&entries, &config.budget);
    render::print_budget(daily, weekly, monthly);
    Ok(())
}

fn cmd_sessions(cli: &Cli, config: &Config, top: usize) -> anyhow::Result<()> {
    let entries = load_and_price(cli, config, false, cli.since, cli.until)?;

    if entries.is_empty() {
        let empty_report = SessionReport {
            generated_at: Utc::now(),
            sessions: Vec::new(),
            total_cost: 0.0,
            total_tokens: 0,
        };
        if cli.json {
            render::print_sessions_json(&empty_report);
        } else if cli.csv {
            render::print_csv_sessions(&empty_report);
        } else {
            println!("No usage data found.");
        }
        return Ok(());
    }

    let mut sessions = rollup::aggregate_by_session(&entries);
    sessions.truncate(top);

    let total_cost: f64 = sessions.iter().map(|s| s.cost).sum();
    let total_tokens: u64 = sessions.iter().map(|s| s.total_tokens).sum();

    let report = SessionReport {
        generated_at: Utc::now(),
        sessions,
        total_cost,
        total_tokens,
    };

    if cli.json {
        render::print_sessions_json(&report);
    } else if cli.csv {
        render::print_csv_sessions(&report);
    } else {
        render::print_sessions_table(&report);
    }

    Ok(())
}

fn cmd_prune(before: NaiveDate) -> anyhow::Result<()> {
    let cache = Cache::open()?;
    let deleted = cache.prune_before(before)?;
    println!("Pruned {deleted} preserved entries with timestamps before {before}.");
    Ok(())
}

// --- Formatting helpers ---

fn format_budget_short(spent: f64, limit: f64) -> String {
    let pct = if limit > 0.0 {
        spent / limit * 100.0
    } else {
        0.0
    };
    if pct > 100.0 {
        format!("OVER ${limit:.0} limit")
    } else {
        format!("{pct:.0}%")
    }
}

fn format_provider_count(count: usize) -> String {
    if count == 1 {
        "1 provider".to_string()
    } else {
        format!("{count} providers")
    }
}

// --- Cache-aware parsing ---

/// Parse entries using cache. Strategy:
/// 1. Get all cached (file, mtime) pairs in one query
/// 2. Discover provider files and check which have changed
/// 3. Only parse changed files, store results in cache
/// 4. Load everything from cache in one bulk query
fn parse_with_cache(
    registry: &SourceSet,
    filter: &[String],
    force_refresh: bool,
    force_reparse: bool,
    since: Option<NaiveDate>,
    until: Option<NaiveDate>,
) -> anyhow::Result<Vec<types::Record>> {
    let mut cache = match Cache::open() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("[tokemon] Warning: cache unavailable ({e}); parsing all files");
            None
        }
    };

    let providers = resolve_source_refs(registry, filter)?;

    let Some(ref mut cache) = cache else {
        return Ok(parse_all_directly(&providers));
    };

    let has_filters = since.is_some() || until.is_some() || !filter.is_empty();

    // If cache is fresh and no --refresh/--reparse flag, skip discovery entirely
    if !force_refresh && !force_reparse && !cache.should_rediscover(REDISCOVERY_INTERVAL_SECS) {
        let mut entries = if has_filters {
            cache.load_entries_filtered(since, until, filter)?
        } else {
            cache.load_all_entries()?
        };
        // Dedup is handled inside load_all_entries / load_entries_filtered.
        entries.sort_by_key(|e| e.timestamp);
        return Ok(entries);
    }

    // When --reparse, ignore cached mtimes so every file gets re-parsed
    let cached_mtimes = if force_reparse {
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
    // otherwise we'd incorrectly mark entries from non-filtered providers)
    if filter.is_empty() {
        cache.mark_preserved(&discovered_files);
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
        cache.load_entries_filtered(since, until, filter)?
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
) -> anyhow::Result<Vec<&'a dyn source::Source>> {
    if filter.is_empty() {
        return Ok(registry.available());
    }

    filter
        .iter()
        .map(|name| {
            registry
                .get(name)
                .ok_or_else(|| error::TokemonError::ProviderNotFound(name.clone()).into())
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
