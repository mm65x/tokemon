use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DisplayMode {
    /// Per-model breakdown rows
    Breakdown,
    /// One row per date compact view
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortOrder {
    /// Oldest first
    Asc,
    /// Newest first
    Desc,
}

#[derive(Parser)]
#[command(
    name = "tokemon",
    version,
    about = "Unified LLM token usage tracking across all providers"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Aggregation frequency: daily, weekly, or monthly
    #[arg(short = 'f', long, global = true, value_enum, default_value = "daily")]
    pub frequency: Frequency,

    /// Output as JSON instead of table
    #[arg(long, global = true)]
    pub json: bool,

    /// Output as CSV
    #[arg(long, global = true, conflicts_with = "json")]
    pub csv: bool,

    /// Display mode: breakdown (per-model) or compact (per-date)
    #[arg(short = 'd', long, global = true, value_enum)]
    pub display: Option<DisplayMode>,

    /// Filter by provider (repeatable: -p claude-code -p codex)
    #[arg(short = 'p', long = "provider", global = true)]
    pub providers: Vec<String>,

    /// Show usage since this date (YYYY-MM-DD)
    #[arg(long, global = true)]
    pub since: Option<NaiveDate>,

    /// Show usage until this date (YYYY-MM-DD)
    #[arg(long, global = true)]
    pub until: Option<NaiveDate>,

    /// Skip cost calculation (faster, shows tokens only)
    #[arg(long, global = true)]
    pub no_cost: bool,

    /// Don't fetch remote pricing data (use cached/offline)
    #[arg(long, global = true)]
    pub offline: bool,

    /// Sort order: asc (oldest first) or desc (newest first)
    #[arg(short = 'o', long, global = true, value_enum)]
    pub order: Option<SortOrder>,

    /// Force re-discovery of files (ignore cache freshness)
    #[arg(long, global = true)]
    pub refresh: bool,

    /// Force re-parse of all files (ignore cached data)
    #[arg(long, global = true)]
    pub reparse: bool,
}

impl Cli {
    /// Resolve display mode from CLI flag and config default
    pub fn display_mode(&self, config: &crate::config::Config) -> DisplayMode {
        self.display.unwrap_or({
            if config.breakdown {
                DisplayMode::Breakdown
            } else {
                DisplayMode::Compact
            }
        })
    }

    /// Whether to use descending sort order
    pub fn is_desc(&self, config: &crate::config::Config) -> bool {
        match self.order {
            Some(SortOrder::Desc) => true,
            Some(SortOrder::Asc) => false,
            None => config.sort_order == crate::config::ConfigSortOrder::Desc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Frequency {
    /// Daily aggregation
    Daily,
    /// Weekly aggregation
    Weekly,
    /// Monthly aggregation
    Monthly,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compact one-line output for shell prompts and status bars
    Statusline,
    /// Show budget progress against configured limits
    Budget,
    /// List auto-detected providers on this machine
    Discover,
    /// Generate default config file at ~/.config/tokemon/config.toml
    Init,
    /// Show per-session cost breakdown
    Sessions {
        /// Show top N sessions by cost
        #[arg(long, default_value = "20")]
        top: usize,
    },
    /// Delete old preserved data from the cache
    Prune {
        /// Delete preserved entries before this date (YYYY-MM-DD)
        #[arg(long)]
        before: NaiveDate,
    },
    /// Start MCP (Model Context Protocol) server over stdio
    Mcp,
    /// Live monitoring dashboard
    Top {
        /// Initial view: today, week, or month
        #[arg(long, default_value = "today")]
        view: String,

        /// Data refresh interval in seconds (0 = use config or default of 2s)
        #[arg(long, default_value = "0")]
        interval: u64,
    },
}
