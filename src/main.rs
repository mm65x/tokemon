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
use cli::{Cli, Commands, Frequency};
use config::Config;
use pipeline::load_and_price;
use source::SourceSet;
use types::{Report, SessionReport};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match &cli.command {
        Commands::Report => cmd_report(&cli, &config),
        Commands::Discover => {
            cmd_discover();
            Ok(())
        }
        Commands::Init => cmd_init(),
        Commands::Statusline => cmd_statusline(&cli, &config),
        Commands::Budget => cmd_budget(&cli, &config),
        Commands::Sessions { top } => cmd_sessions(&cli, &config, *top),
        Commands::Prune { before } => cmd_prune(*before),
        Commands::Mcp => mcp::run(&cli, &config),
        Commands::Top { view, interval } => {
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

// --- Shared helpers for command handlers ---

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

// --- Command handlers ---

fn cmd_report(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let freq = cli.frequency;
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
            let pipeline_opts = pipeline::PipelineOptions::from_cli_config(cli, config);
            if pipeline_opts.providers.is_empty() {
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
        let status = pacemaker::evaluate(&entries, &config.budget);
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
    let status = pacemaker::evaluate(&entries, &config.budget);
    render::print_budget(&status);
    Ok(())
}

fn cmd_sessions(cli: &Cli, config: &Config, top: usize) -> anyhow::Result<()> {
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
