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
mod pipeline;
mod render;
mod rollup;
mod source;
mod timestamp;
mod tui;
mod types;

use cache::Cache;
use cli::{Cli, Commands, Frequency, OutputFormat, SourceCommands};
use config::{Config, TimezoneMode};
use pipeline::load_and_price;
use source::{Source, SourceSet};
use types::{Report, SessionReport, StatusCapabilities, StatusReport, StatusScope};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match &cli.command {
        Commands::Report => cmd_report(&cli, &config),
        Commands::Status => cmd_status(&cli, &config),
        Commands::Discover => {
            cmd_discover();
            Ok(())
        }
        Commands::Init => cmd_init(),
        Commands::Source { command } => cmd_source(command),
        Commands::Config => tui::run_config(&config, cli.offline || config.offline),
        Commands::Statusline => cmd_statusline(&cli, &config),
        Commands::Budget => cmd_budget(&cli, &config),
        Commands::Sessions { top } => cmd_sessions(&cli, &config, *top),
        Commands::Prune { before } => cmd_prune(*before),
        Commands::Mcp => mcp::run(&cli, &config),
        Commands::Top { view, interval } => {
            let (effective_config, tick, offline) = resolve_top_config(&cli, &config, *interval);
            tui::run(&effective_config, view, tick, offline)
        }
    }
}

fn cmd_discover() {
    let registry = SourceSet::new();

    let info: Vec<crate::types::ProviderInfo> = registry
        .all()
        .iter()
        .map(|p| {
            let available = !p.discover_files().is_empty();
            let data_dir = p.data_dir().display().to_string();
            let file_count = if available {
                p.discover_files().len()
            } else {
                0
            };
            crate::types::ProviderInfo {
                name: p.name().to_string(),
                display_name: p.display_name().to_string(),
                available,
                data_dir,
                file_count,
            }
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

fn cmd_source(command: &SourceCommands) -> anyhow::Result<()> {
    match command {
        SourceCommands::Configure => {
            tui::run_source_config()?;
            Ok(())
        }
        SourceCommands::Validate { definition } => {
            let source = source::custom::CustomSource::from_file(definition)?;
            let files = source.discover_files();
            println!("Valid custom source: {}", source.display_name());
            println!("Schema version: {}", source.definition().schema_version);
            println!("Discovered files: {}", files.len());
            if let Some(sample) = files.first() {
                let records = source.parse_file(sample)?;
                println!("Preview file: {}", sample.display());
                println!("Parsed records: {}", records.len());
                for record in records.iter().take(5) {
                    println!(
                        "  {} | model={} | input={} | output={} | cache_read={} | cache_write={} | thinking={}",
                        record.timestamp.to_rfc3339(),
                        record.model.as_deref().unwrap_or("unknown"),
                        record.input_tokens,
                        record.output_tokens,
                        record.cache_read_tokens,
                        record.cache_creation_tokens,
                        record.thinking_tokens,
                    );
                }
            } else {
                println!("No matching files found.");
            }
            Ok(())
        }
    }
}

// --- Shared helpers for command handlers ---

/// Compute the start date for a given frequency.
#[must_use]
fn frequency_since(freq: Frequency, tz: TimezoneMode) -> NaiveDate {
    match freq {
        Frequency::Daily => timestamp::today_in(tz),
        Frequency::Weekly => timestamp::start_of_week_in(tz),
        Frequency::Monthly => timestamp::start_of_month_in(tz),
    }
}

/// Resolve effective TUI config, tick interval, and offline mode from CLI + config.
#[must_use]
fn resolve_top_config(cli: &Cli, config: &Config, interval: u64) -> (Config, u64, bool) {
    let mut effective_config = config.clone();
    if !cli.providers.is_empty() {
        effective_config.providers.clone_from(&cli.providers);
    }
    if cli.no_cost {
        effective_config.no_cost = true;
    }
    if cli.refresh {
        effective_config.refresh = true;
    }
    if cli.reparse {
        effective_config.reparse = true;
    }
    // CLI --interval overrides config, config overrides hardcoded default
    let tick = if interval > 0 {
        interval
    } else {
        effective_config.tick_interval
    };
    let offline = cli.offline || effective_config.offline;
    (effective_config, tick, offline)
}

/// Merge two optional dates, taking the later of the two.
#[must_use]
fn merge_since(a: Option<NaiveDate>, b: Option<NaiveDate>) -> Option<NaiveDate> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

// --- Command handlers ---

fn cmd_report(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let freq = cli.frequency(config);
    let output_format = cli.output_format(config);
    let period = match freq {
        Frequency::Daily => "daily",
        Frequency::Weekly => "weekly",
        Frequency::Monthly => "monthly",
    };
    let entries = load_and_price(
        &pipeline::PipelineOptions::from_cli_config(cli, config),
        false,
    )?;

    if entries.is_empty() {
        let empty_report = Report {
            period: period.to_string(),
            generated_at: Utc::now(),
            providers_found: Vec::new(),
            summaries: Vec::new(),
            total_cost: 0.0,
            total_tokens: 0,
        };
        match output_format {
            OutputFormat::Json => render::print_json(&empty_report),
            OutputFormat::Csv => {
                let breakdown = cli.display_mode(config) == cli::DisplayMode::Breakdown;
                if breakdown {
                    render::print_csv_breakdown(&empty_report);
                } else {
                    render::print_csv_compact(&empty_report);
                }
            }
            OutputFormat::Table => {
                println!("No usage data found.");
                let pipeline_opts = pipeline::PipelineOptions::from_cli_config(cli, config);
                if pipeline_opts.providers.is_empty() {
                    println!("Run `tokemon discover` to see which providers are available.");
                }
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
        Frequency::Weekly => rollup::aggregate_weekly_in(&entries, config.timezone),
        Frequency::Monthly => rollup::aggregate_monthly_in(&entries, config.timezone),
        Frequency::Daily => rollup::aggregate_daily_in(&entries, config.timezone),
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

    match output_format {
        OutputFormat::Json => render::print_json(&report),
        OutputFormat::Csv => {
            let breakdown = cli.display_mode(config) == cli::DisplayMode::Breakdown;
            if breakdown {
                render::print_csv_breakdown(&report);
            } else {
                render::print_csv_compact(&report);
            }
        }
        OutputFormat::Table => {
            let breakdown = cli.display_mode(config) == cli::DisplayMode::Breakdown;
            render::print_table(&report, breakdown, &config.columns);
        }
    }

    Ok(())
}

fn cmd_status(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let freq = cli.frequency(config);
    let pipeline::LoadedEntries {
        entries,
        pricing_available,
    } = pipeline::load_and_price_with_status(
        &pipeline::PipelineOptions::from_cli_config(cli, config),
        false,
    )?;

    let mut summaries = match freq {
        Frequency::Weekly => rollup::aggregate_weekly_in(&entries, config.timezone),
        Frequency::Monthly => rollup::aggregate_monthly_in(&entries, config.timezone),
        Frequency::Daily => rollup::aggregate_daily_in(&entries, config.timezone),
    };
    summaries.sort_unstable_by_key(|summary| summary.date);

    let providers: Vec<String> = entries
        .iter()
        .map(|entry| entry.provider.to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let total_cost = summaries.iter().map(|summary| summary.total_cost).sum();
    let total_tokens = entries.iter().map(types::Record::total_tokens).sum();
    let total_requests = summaries.iter().map(|summary| summary.total_requests).sum();

    let status = StatusReport {
        schema_version: 1,
        generated_at: Utc::now(),
        state: if entries.is_empty() {
            "empty".to_string()
        } else {
            "populated".to_string()
        },
        scope: StatusScope {
            frequency: frequency_label(freq).to_string(),
            since: cli.since,
            until: cli.until,
        },
        providers,
        summaries,
        total_cost,
        total_tokens,
        total_requests,
        capabilities: StatusCapabilities {
            cost: pricing_available,
            date_filters: true,
            provider_filters: true,
            periodic_summaries: true,
            session_view: true,
        },
    };

    render::print_status_json(&status);
    Ok(())
}

fn cmd_statusline(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let freq = cli.frequency(config);
    let since = frequency_since(freq, config.timezone);
    let effective_since = merge_since(cli.since, Some(since));
    let entries = load_and_price(
        &pipeline::PipelineOptions {
            since: effective_since,
            ..pipeline::PipelineOptions::from_cli_config(cli, config)
        },
        true,
    )?;
    let period_label = match freq {
        Frequency::Daily => "today",
        Frequency::Weekly => "this week",
        Frequency::Monthly => "this month",
    };

    let mut providers_seen = std::collections::HashSet::new();
    let (total_cost, total_tokens) = entries
        .iter()
        .filter(|e| {
            let d = timestamp::date_for(e.timestamp, config.timezone);
            effective_since.is_none_or(|s| d >= s) && cli.until.is_none_or(|u| d <= u)
        })
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
        let status = pacemaker::evaluate_in(&entries, &config.budget, config.timezone);
        match freq {
            Frequency::Daily => status.daily.map(|b| format_budget_short(b.spent, b.limit)),
            Frequency::Weekly => status.weekly.map(|b| format_budget_short(b.spent, b.limit)),
            Frequency::Monthly => status
                .monthly
                .map(|b| format_budget_short(b.spent, b.limit)),
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
    let entries = load_and_price(
        &pipeline::PipelineOptions::from_cli_config(cli, config),
        false,
    )?;
    let status = pacemaker::evaluate_in(&entries, &config.budget, config.timezone);
    render::print_budget(&status);
    Ok(())
}

fn cmd_sessions(cli: &Cli, config: &Config, top: usize) -> anyhow::Result<()> {
    let output_format = cli.output_format(config);
    let entries = load_and_price(
        &pipeline::PipelineOptions::from_cli_config(cli, config),
        false,
    )?;

    if entries.is_empty() {
        let empty_report = SessionReport {
            generated_at: Utc::now(),
            sessions: Vec::new(),
            total_cost: 0.0,
            total_tokens: 0,
        };
        match output_format {
            OutputFormat::Json => render::print_sessions_json(&empty_report),
            OutputFormat::Csv => render::print_csv_sessions(&empty_report),
            OutputFormat::Table => println!("No usage data found."),
        }
        return Ok(());
    }

    let mut sessions = rollup::aggregate_by_session_in(&entries, config.timezone);
    sessions.truncate(top);

    let total_cost: f64 = sessions.iter().map(|s| s.cost).sum();
    let total_tokens: u64 = sessions.iter().map(|s| s.total_tokens).sum();

    let report = SessionReport {
        generated_at: Utc::now(),
        sessions,
        total_cost,
        total_tokens,
    };

    match output_format {
        OutputFormat::Json => render::print_sessions_json(&report),
        OutputFormat::Csv => render::print_csv_sessions(&report),
        OutputFormat::Table => render::print_sessions_table(&report),
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

fn frequency_label(freq: Frequency) -> &'static str {
    match freq {
        Frequency::Daily => "daily",
        Frequency::Weekly => "weekly",
        Frequency::Monthly => "monthly",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_top_config_forwards_global_cli_flags() {
        let mut config = Config::default();
        config.tick_interval = 5;

        let cli = Cli::try_parse_from([
            "tokemon",
            "-p",
            "codex",
            "-p",
            "antigravity",
            "--no-cost",
            "--offline",
            "--refresh",
            "--reparse",
            "top",
            "--interval",
            "10",
        ])
        .expect("parse top cli");

        let (effective, tick, offline) = resolve_top_config(&cli, &config, 10);
        assert_eq!(effective.providers, ["codex", "antigravity"]);
        assert!(effective.no_cost);
        assert!(effective.refresh);
        assert!(effective.reparse);
        assert_eq!(tick, 10);
        assert!(offline);
    }

    #[test]
    fn resolve_top_config_falls_back_to_config_when_flags_omitted() {
        let mut config = Config::default();
        config.providers = vec!["gemini".to_string()];
        config.tick_interval = 7;
        config.offline = true;

        let cli = Cli::try_parse_from(["tokemon", "top"]).expect("parse top cli");
        let (effective, tick, offline) = resolve_top_config(&cli, &config, 0);
        assert_eq!(effective.providers, ["gemini"]);
        assert!(!effective.no_cost);
        assert_eq!(tick, 7);
        assert!(offline);
    }
}
