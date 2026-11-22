use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

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
    pub command: Commands,

    /// Aggregation frequency: daily, weekly, or monthly
    #[arg(short = 'f', long, global = true, value_enum)]
    pub frequency: Option<Frequency>,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

impl Cli {
    /// Resolve aggregation frequency from CLI flag and config default.
    #[must_use]
    pub fn frequency(&self, config: &crate::config::Config) -> Frequency {
        self.frequency.unwrap_or(match config.default_command {
            crate::config::DefaultCommand::Daily => Frequency::Daily,
            crate::config::DefaultCommand::Weekly => Frequency::Weekly,
            crate::config::DefaultCommand::Monthly => Frequency::Monthly,
        })
    }

    /// Resolve output format from CLI flags and config default.
    #[must_use]
    pub fn output_format(&self, config: &crate::config::Config) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else if self.csv {
            OutputFormat::Csv
        } else {
            match config.default_format.to_ascii_lowercase().as_str() {
                "json" => OutputFormat::Json,
                "csv" => OutputFormat::Csv,
                _ => OutputFormat::Table,
            }
        }
    }

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
    /// Generate a static usage report (table, json, or csv)
    Report,
    /// Emit a versioned machine-readable status document
    Status,
    /// Compact one-line output for shell prompts and status bars
    Statusline,
    /// Show budget progress against configured limits
    Budget,
    /// List auto-detected providers on this machine
    Discover,
    /// Generate default config file at ~/.config/tokemon/config.toml
    Init,
    /// Manage declarative local usage sources
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Open the interactive configuration editor
    Config,
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

#[derive(Subcommand)]
pub enum SourceCommands {
    /// Open the guided editor for user-defined JSONL sources
    Configure,
    /// Validate a definition and preview discovered records
    Validate {
        /// TOML definition file to validate
        definition: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, DefaultCommand};

    #[test]
    fn cli_frequency_falls_back_to_config_default_command() {
        let mut config = Config::default();
        config.default_command = DefaultCommand::Weekly;

        let cli = Cli::try_parse_from(["tokemon", "report"]).expect("parse cli");
        assert_eq!(cli.frequency(&config), Frequency::Weekly);

        let override_cli =
            Cli::try_parse_from(["tokemon", "report", "-f", "monthly"]).expect("parse cli");
        assert_eq!(override_cli.frequency(&config), Frequency::Monthly);
    }

    #[test]
    fn cli_output_format_falls_back_to_config_default_format() {
        let mut config = Config::default();
        config.default_format = "json".to_string();

        let cli = Cli::try_parse_from(["tokemon", "report"]).expect("parse cli");
        assert_eq!(cli.output_format(&config), OutputFormat::Json);

        let csv_cli = Cli::try_parse_from(["tokemon", "report", "--csv"]).expect("parse cli");
        assert_eq!(csv_cli.output_format(&config), OutputFormat::Csv);
    }
}
