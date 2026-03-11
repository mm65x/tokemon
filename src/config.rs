use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths;

const CONFIG_FILENAME: &str = "config.toml";

/// Default subcommand when none is specified on the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultCommand {
    Daily,
    Weekly,
    Monthly,
}

impl DefaultCommand {
    /// Cycle to the next value (used by the TUI settings editor).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Daily => Self::Weekly,
            Self::Weekly => Self::Monthly,
            Self::Monthly => Self::Daily,
        }
    }
}

impl std::fmt::Display for DefaultCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daily => f.write_str("daily"),
            Self::Weekly => f.write_str("weekly"),
            Self::Monthly => f.write_str("monthly"),
        }
    }
}

/// Sort order for CLI table output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigSortOrder {
    Asc,
    Desc,
}

impl ConfigSortOrder {
    /// Cycle to the next value (used by the TUI settings editor).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

impl std::fmt::Display for ConfigSortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Asc => f.write_str("asc"),
            Self::Desc => f.write_str("desc"),
        }
    }
}

/// Which metric to use for sparkline trendlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SparklineMetric {
    Tokens,
    Cost,
}

impl SparklineMetric {
    /// Cycle to the next value (used by the TUI settings editor).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Tokens => Self::Cost,
            Self::Cost => Self::Tokens,
        }
    }
}

impl std::fmt::Display for SparklineMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tokens => f.write_str("tokens"),
            Self::Cost => f.write_str("cost"),
        }
    }
}

/// User configuration for tokemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default subcommand when none specified
    pub default_command: DefaultCommand,

    /// Default output format: "table" or "json"
    pub default_format: String,

    /// Whether to show per-model breakdown by default (like --breakdown)
    pub breakdown: bool,

    /// Skip cost calculation by default
    pub no_cost: bool,

    /// Use offline pricing by default
    pub offline: bool,

    /// Default providers to show (empty = all available)
    pub providers: Vec<String>,

    /// Column visibility settings
    pub columns: ColumnConfig,

    /// Sort order for CLI table output
    pub sort_order: ConfigSortOrder,

    /// Always re-discover files (ignore cache freshness)
    pub refresh: bool,

    /// Always re-parse all files from disk (ignore cached data)
    pub reparse: bool,

    /// Budget limits for pacemaker
    pub budget: BudgetConfig,

    /// Polling interval for `tokemon top` in seconds (0 = use default of 2s)
    pub tick_interval: u64,

    /// Show sparkline trendlines in summary cards
    pub show_sparklines: bool,

    /// Sparkline metric for trendlines
    pub sparkline_metric: SparklineMetric,

    /// Today sparkline bucket size in minutes (default: 10)
    pub today_bucket_mins: u64,

    /// This Week sparkline bucket size in hours (default: 4)
    pub week_bucket_hours: u64,

    /// This Month sparkline bucket size in days (default: 1)
    pub month_bucket_days: u64,
}

/// Budget limits for the pacemaker system (all in USD)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct BudgetConfig {
    /// Daily spending limit
    pub daily: Option<f64>,
    /// Weekly spending limit
    pub weekly: Option<f64>,
    /// Monthly spending limit
    pub monthly: Option<f64>,
}

/// Which columns to display in table output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColumnConfig {
    pub date: bool,
    pub model: bool,
    pub api_provider: bool,
    pub client: bool,
    pub input: bool,
    pub output: bool,
    pub cache_write: bool,
    pub cache_read: bool,
    pub total_tokens: bool,
    pub cost: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_command: DefaultCommand::Daily,
            default_format: "table".to_string(),
            breakdown: false,
            no_cost: false,
            offline: false,
            providers: Vec::new(),
            columns: ColumnConfig::default(),
            sort_order: ConfigSortOrder::Asc,
            refresh: false,
            reparse: false,
            budget: BudgetConfig::default(),
            tick_interval: 0,
            show_sparklines: true,
            sparkline_metric: SparklineMetric::Tokens,
            today_bucket_mins: 10,
            week_bucket_hours: 4,
            month_bucket_days: 1,
        }
    }
}

impl Default for ColumnConfig {
    fn default() -> Self {
        Self {
            date: true,
            model: true,
            api_provider: true,
            client: true,
            input: true,
            output: true,
            cache_write: true,
            cache_read: true,
            total_tokens: true,
            cost: true,
        }
    }
}

impl Config {
    /// Load config from ~/.config/tokemon/config.toml, falling back to defaults
    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<Config>(&content) {
                Ok(config) => config.validated(),
                Err(e) => {
                    eprintln!(
                        "[tokemon] Warning: failed to parse {}: {}; using defaults",
                        path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Validate config values, replacing invalid ones with defaults.
    ///
    /// `default_command`, `sort_order`, and `sparkline_metric` are enums
    /// with `#[serde(rename_all = "lowercase")]`, so invalid TOML values
    /// are caught at deserialization time and never reach this method.
    fn validated(mut self) -> Self {
        let defaults = Self::default();

        if !matches!(self.default_format.as_str(), "table" | "json") {
            eprintln!(
                "[tokemon] Warning: invalid default_format '{}'; using '{}'",
                self.default_format, defaults.default_format
            );
            self.default_format = defaults.default_format;
        }

        // Clamp bucket sizes to sensible ranges
        if self.today_bucket_mins == 0 || self.today_bucket_mins > 60 {
            self.today_bucket_mins = defaults.today_bucket_mins;
        }
        if self.week_bucket_hours == 0 || self.week_bucket_hours > 24 {
            self.week_bucket_hours = defaults.week_bucket_hours;
        }
        if self.month_bucket_days == 0 || self.month_bucket_days > 7 {
            self.month_bucket_days = defaults.month_bucket_days;
        }

        if self.tick_interval > 300 {
            eprintln!(
                "[tokemon] Warning: tick_interval {} exceeds maximum (300s); clamping",
                self.tick_interval
            );
            self.tick_interval = 300;
        }

        self
    }

    /// Write the default config to disk (for `tokemon init`)
    pub fn write_default() -> anyhow::Result<PathBuf> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let default = Self::default();
        let content = toml::to_string_pretty(&default)?;
        let header = "# Tokemon configuration\n\
                      # Location: ~/.config/tokemon/config.toml\n\
                      #\n\
                      # Changes here affect default behavior.\n\
                      # CLI flags always override config values.\n\n";
        fs::write(&path, format!("{header}{content}"))?;
        Ok(path)
    }

    /// Save the current config to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        let header = "# Tokemon configuration\n\
                      # Location: ~/.config/tokemon/config.toml\n\
                      #\n\
                      # Changes here affect default behavior.\n\
                      # CLI flags always override config values.\n\n";
        fs::write(&path, format!("{header}{content}"))?;
        Ok(())
    }

    pub fn config_path() -> PathBuf {
        let config_dir = directories::ProjectDirs::from("", "", "tokemon").map_or_else(
            || paths::home_dir().join(".config/tokemon"),
            |d| d.config_dir().to_path_buf(),
        );
        config_dir.join(CONFIG_FILENAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.default_command, DefaultCommand::Daily);
        assert_eq!(config.default_format, "table");
        assert!(!config.breakdown);
        assert!(!config.no_cost);
        assert!(!config.offline);
        assert!(config.providers.is_empty());
        assert_eq!(config.sort_order, ConfigSortOrder::Asc);
        assert!(!config.refresh);
        assert!(!config.reparse);
        assert_eq!(config.tick_interval, 0);
        assert!(config.show_sparklines);
        assert_eq!(config.sparkline_metric, SparklineMetric::Tokens);
        assert_eq!(config.today_bucket_mins, 10);
        assert_eq!(config.week_bucket_hours, 4);
        assert_eq!(config.month_bucket_days, 1);

        assert!(config.columns.date);
        assert!(config.columns.model);
        assert!(config.columns.api_provider);
        assert!(config.columns.client);
        assert!(config.columns.input);
        assert!(config.columns.output);
        assert!(config.columns.cache_write);
        assert!(config.columns.cache_read);
        assert!(config.columns.total_tokens);
        assert!(config.columns.cost);

        assert!(config.budget.daily.is_none());
        assert!(config.budget.weekly.is_none());
        assert!(config.budget.monthly.is_none());
    }

    #[test]
    fn test_default_command_enum() {
        assert_eq!(DefaultCommand::Daily.next(), DefaultCommand::Weekly);
        assert_eq!(DefaultCommand::Weekly.next(), DefaultCommand::Monthly);
        assert_eq!(DefaultCommand::Monthly.next(), DefaultCommand::Daily);

        assert_eq!(DefaultCommand::Daily.to_string(), "daily");
        assert_eq!(DefaultCommand::Weekly.to_string(), "weekly");
        assert_eq!(DefaultCommand::Monthly.to_string(), "monthly");
    }

    #[test]
    fn test_config_sort_order_enum() {
        assert_eq!(ConfigSortOrder::Asc.next(), ConfigSortOrder::Desc);
        assert_eq!(ConfigSortOrder::Desc.next(), ConfigSortOrder::Asc);

        assert_eq!(ConfigSortOrder::Asc.to_string(), "asc");
        assert_eq!(ConfigSortOrder::Desc.to_string(), "desc");
    }

    #[test]
    fn test_sparkline_metric_enum() {
        assert_eq!(SparklineMetric::Tokens.next(), SparklineMetric::Cost);
        assert_eq!(SparklineMetric::Cost.next(), SparklineMetric::Tokens);

        assert_eq!(SparklineMetric::Tokens.to_string(), "tokens");
        assert_eq!(SparklineMetric::Cost.to_string(), "cost");
    }

    #[test]
    fn test_config_validated() {
        let mut config = Config::default();
        config.default_format = "invalid".to_string();
        config.today_bucket_mins = 0;
        config.week_bucket_hours = 25;
        config.month_bucket_days = 8;
        config.tick_interval = 400;

        let validated = config.validated();
        assert_eq!(validated.default_format, "table"); // Reset to default
        assert_eq!(validated.today_bucket_mins, 10);
        assert_eq!(validated.week_bucket_hours, 4);
        assert_eq!(validated.month_bucket_days, 1);
        assert_eq!(validated.tick_interval, 300); // Clamped to 300
    }
}
