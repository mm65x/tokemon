use std::time::Instant;

use crate::config::Config;

// ── Settings state ────────────────────────────────────────────────────────

/// Which field is being displayed/edited in the settings view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingField {
    TickInterval,
    DefaultCommand,
    DefaultFormat,
    Breakdown,
    NoCost,
    Offline,
    Refresh,
    Reparse,
    SortOrder,
    ShowSparklines,
    SparklineMetric,
    TodayBucketMins,
    WeekBucketHours,
    MonthBucketDays,
    BudgetDaily,
    BudgetWeekly,
    BudgetMonthly,
    Providers,
    ColDate,
    ColModel,
    ColApiProvider,
    ColClient,
    ColInput,
    ColOutput,
    ColCacheWrite,
    ColCacheRead,
    ColRequests,
    ColTotalTokens,
    ColCost,
}

impl SettingField {
    /// Total number of settings fields.
    pub const COUNT: usize = 29;

    /// All fields in display order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::TickInterval,
        Self::DefaultCommand,
        Self::DefaultFormat,
        Self::Breakdown,
        Self::NoCost,
        Self::Offline,
        Self::Refresh,
        Self::Reparse,
        Self::SortOrder,
        Self::ShowSparklines,
        Self::SparklineMetric,
        Self::TodayBucketMins,
        Self::WeekBucketHours,
        Self::MonthBucketDays,
        Self::BudgetDaily,
        Self::BudgetWeekly,
        Self::BudgetMonthly,
        Self::Providers,
        Self::ColDate,
        Self::ColModel,
        Self::ColApiProvider,
        Self::ColClient,
        Self::ColInput,
        Self::ColOutput,
        Self::ColCacheWrite,
        Self::ColCacheRead,
        Self::ColRequests,
        Self::ColTotalTokens,
        Self::ColCost,
    ];

    /// Display label for this field.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TickInterval => "Tick Interval (s) *",
            Self::DefaultCommand => "Default Command",
            Self::DefaultFormat => "Default Format",
            Self::Breakdown => "Model Breakdown",
            Self::NoCost => "Disable Costs",
            Self::Offline => "Offline Pricing",
            Self::Refresh => "Always Refresh",
            Self::Reparse => "Always Reparse",
            Self::SortOrder => "Sort Order",
            Self::ShowSparklines => "Show Sparklines",
            Self::SparklineMetric => "Sparkline Metric",
            Self::TodayBucketMins => "Today Bar (mins)",
            Self::WeekBucketHours => "Week Bar (hours)",
            Self::MonthBucketDays => "Month Bar (days)",
            Self::BudgetDaily => "Daily Budget ($)",
            Self::BudgetWeekly => "Weekly Budget ($)",
            Self::BudgetMonthly => "Monthly Budget ($)",
            Self::Providers => "Providers (comma-separated)",
            Self::ColDate => "Date",
            Self::ColModel => "Model",
            Self::ColApiProvider => "API Provider",
            Self::ColClient => "Client",
            Self::ColInput => "Input Tokens",
            Self::ColOutput => "Output Tokens",
            Self::ColCacheWrite => "Cache Write",
            Self::ColCacheRead => "Cache Read",
            Self::ColRequests => "Requests",
            Self::ColTotalTokens => "Total Tokens",
            Self::ColCost => "Cost",
        }
    }

    /// Section header for visual grouping (returns Some for the first item in each section).
    #[must_use]
    pub fn section_header(self) -> Option<&'static str> {
        match self {
            Self::TickInterval => Some("General"),
            Self::DefaultCommand => Some("Defaults"),
            Self::ShowSparklines => Some("Sparklines"),
            Self::BudgetDaily => Some("Budget Limits"),
            Self::Providers => Some("Sources"),
            Self::ColDate => Some("Columns"),
            _ => None,
        }
    }

    /// Whether this field is a boolean toggle.
    #[must_use]
    pub fn is_bool(self) -> bool {
        matches!(
            self,
            Self::Breakdown
                | Self::NoCost
                | Self::Offline
                | Self::Refresh
                | Self::Reparse
                | Self::ShowSparklines
                | Self::ColDate
                | Self::ColModel
                | Self::ColApiProvider
                | Self::ColClient
                | Self::ColInput
                | Self::ColOutput
                | Self::ColCacheWrite
                | Self::ColCacheRead
                | Self::ColRequests
                | Self::ColTotalTokens
                | Self::ColCost
        )
    }

    /// Whether this field is an enum that cycles through values.
    #[must_use]
    pub fn is_enum(self) -> bool {
        matches!(
            self,
            Self::DefaultCommand | Self::DefaultFormat | Self::SortOrder | Self::SparklineMetric
        )
    }

    /// Whether this field accepts free-form text input.
    #[must_use]
    pub fn is_text(self) -> bool {
        matches!(self, Self::Providers)
    }

    /// Get the current value as a display string from a config.
    #[must_use]
    pub fn display_value(self, config: &Config) -> String {
        match self {
            Self::TickInterval => {
                let v = config.tick_interval;
                if v == 0 {
                    "2 (default)".to_string()
                } else {
                    v.to_string()
                }
            }
            Self::DefaultCommand => config.default_command.to_string(),
            Self::DefaultFormat => config.default_format.clone(),
            Self::Breakdown => bool_display(config.breakdown),
            Self::NoCost => bool_display(config.no_cost),
            Self::Offline => bool_display(config.offline),
            Self::Refresh => bool_display(config.refresh),
            Self::Reparse => bool_display(config.reparse),
            Self::SortOrder => config.sort_order.to_string(),
            Self::ShowSparklines => if config.show_sparklines { "Yes" } else { "No" }.to_string(),
            Self::SparklineMetric => config.sparkline_metric.to_string(),
            Self::TodayBucketMins => config.today_bucket_mins.to_string(),
            Self::WeekBucketHours => config.week_bucket_hours.to_string(),
            Self::MonthBucketDays => config.month_bucket_days.to_string(),
            Self::BudgetDaily => config
                .budget
                .daily
                .map_or("--".to_string(), |v| format!("{v:.2}")),
            Self::BudgetWeekly => config
                .budget
                .weekly
                .map_or("--".to_string(), |v| format!("{v:.2}")),
            Self::BudgetMonthly => config
                .budget
                .monthly
                .map_or("--".to_string(), |v| format!("{v:.2}")),
            Self::Providers => {
                if config.providers.is_empty() {
                    "(all)".to_string()
                } else {
                    config.providers.join(", ")
                }
            }
            Self::ColDate => bool_display(config.columns.date),
            Self::ColModel => bool_display(config.columns.model),
            Self::ColApiProvider => bool_display(config.columns.api_provider),
            Self::ColClient => bool_display(config.columns.client),
            Self::ColInput => bool_display(config.columns.input),
            Self::ColOutput => bool_display(config.columns.output),
            Self::ColCacheWrite => bool_display(config.columns.cache_write),
            Self::ColCacheRead => bool_display(config.columns.cache_read),
            Self::ColRequests => bool_display(config.columns.requests),
            Self::ColTotalTokens => bool_display(config.columns.total_tokens),
            Self::ColCost => bool_display(config.columns.cost),
        }
    }

    /// Toggle a boolean field on the given config. No-op for non-bool fields.
    pub fn toggle_bool(self, config: &mut Config) {
        match self {
            Self::Breakdown => config.breakdown = !config.breakdown,
            Self::NoCost => config.no_cost = !config.no_cost,
            Self::Offline => config.offline = !config.offline,
            Self::Refresh => config.refresh = !config.refresh,
            Self::Reparse => config.reparse = !config.reparse,
            Self::ShowSparklines => config.show_sparklines = !config.show_sparklines,
            Self::ColDate => config.columns.date = !config.columns.date,
            Self::ColModel => config.columns.model = !config.columns.model,
            Self::ColApiProvider => config.columns.api_provider = !config.columns.api_provider,
            Self::ColClient => config.columns.client = !config.columns.client,
            Self::ColInput => config.columns.input = !config.columns.input,
            Self::ColOutput => config.columns.output = !config.columns.output,
            Self::ColCacheWrite => config.columns.cache_write = !config.columns.cache_write,
            Self::ColCacheRead => config.columns.cache_read = !config.columns.cache_read,
            Self::ColRequests => config.columns.requests = !config.columns.requests,
            Self::ColTotalTokens => config.columns.total_tokens = !config.columns.total_tokens,
            Self::ColCost => config.columns.cost = !config.columns.cost,
            _ => {}
        }
    }

    /// Cycle an enum field to its next value. No-op for non-enum fields.
    pub fn cycle_enum(self, config: &mut Config) {
        match self {
            Self::DefaultCommand => {
                config.default_command = config.default_command.next();
            }
            Self::DefaultFormat => {
                config.default_format = if config.default_format == "json" {
                    "table".to_string()
                } else {
                    "json".to_string()
                };
            }
            Self::SortOrder => {
                config.sort_order = config.sort_order.next();
            }
            Self::SparklineMetric => {
                config.sparkline_metric = config.sparkline_metric.next();
            }
            _ => {}
        }
    }

    /// Apply a string value from the edit buffer to the config.
    /// Returns an error when the value is outside the accepted range.
    pub fn apply_value(self, config: &mut Config, value: &str) -> Result<(), &'static str> {
        match self {
            Self::TickInterval => {
                let v = value
                    .parse::<u64>()
                    .map_err(|_| "Enter a whole number from 0 to 300")?;
                if v > 300 {
                    return Err("Tick interval must be between 0 and 300 seconds");
                }
                config.tick_interval = v;
                Ok(())
            }
            Self::TodayBucketMins => {
                let v = value
                    .parse::<u64>()
                    .map_err(|_| "Enter a whole number from 1 to 60")?;
                if !(1..=60).contains(&v) {
                    return Err("Today bucket must be between 1 and 60 minutes");
                }
                config.today_bucket_mins = v;
                Ok(())
            }
            Self::WeekBucketHours => {
                let v = value
                    .parse::<u64>()
                    .map_err(|_| "Enter a whole number from 1 to 24")?;
                if !(1..=24).contains(&v) {
                    return Err("Week bucket must be between 1 and 24 hours");
                }
                config.week_bucket_hours = v;
                Ok(())
            }
            Self::MonthBucketDays => {
                let v = value
                    .parse::<u64>()
                    .map_err(|_| "Enter a whole number from 1 to 7")?;
                if !(1..=7).contains(&v) {
                    return Err("Month bucket must be between 1 and 7 days");
                }
                config.month_bucket_days = v;
                Ok(())
            }
            Self::BudgetDaily => apply_budget_value(&mut config.budget.daily, value),
            Self::BudgetWeekly => apply_budget_value(&mut config.budget.weekly, value),
            Self::BudgetMonthly => apply_budget_value(&mut config.budget.monthly, value),
            Self::Providers => {
                config.providers = value
                    .split(',')
                    .map(str::trim)
                    .filter(|provider| !provider.is_empty())
                    .map(str::to_string)
                    .collect();
                Ok(())
            }
            _ => Err("This setting is not text-editable"),
        }
    }

    /// Get the raw edit value (for pre-populating the edit buffer).
    #[must_use]
    pub fn edit_value(self, config: &Config) -> String {
        match self {
            Self::TickInterval => config.tick_interval.to_string(),
            Self::TodayBucketMins => config.today_bucket_mins.to_string(),
            Self::WeekBucketHours => config.week_bucket_hours.to_string(),
            Self::MonthBucketDays => config.month_bucket_days.to_string(),
            Self::BudgetDaily => config
                .budget
                .daily
                .map_or(String::new(), |v| format!("{v:.2}")),
            Self::BudgetWeekly => config
                .budget
                .weekly
                .map_or(String::new(), |v| format!("{v:.2}")),
            Self::BudgetMonthly => config
                .budget
                .monthly
                .map_or(String::new(), |v| format!("{v:.2}")),
            Self::Providers => config.providers.join(", "),
            _ => String::new(),
        }
    }
}

pub(crate) fn bool_display(v: bool) -> String {
    if v { "Yes" } else { "No" }.to_string()
}

fn apply_budget_value(target: &mut Option<f64>, value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        *target = None;
        return Ok(());
    }
    let v = value
        .parse::<f64>()
        .map_err(|_| "Enter a positive number or leave blank")?;
    if v > 0.0 && v.is_finite() {
        *target = Some(v);
        Ok(())
    } else {
        Err("Budget must be a positive finite number")
    }
}

/// Interactive settings editor state.
pub(crate) struct SettingsState {
    /// Working copy of config — edits happen here.
    pub draft: Config,
    /// Whether the draft differs from the saved config.
    pub unsaved: bool,
    /// Currently selected field index.
    pub selected: usize,
    /// Whether we're currently editing a text/numeric field.
    pub editing: bool,
    /// Whether the user is reviewing a save before it is written.
    pub confirming_save: bool,
    /// Text buffer for the field being edited.
    pub edit_buffer: String,
    /// Brief confirmation message (e.g. "Saved!"), with the instant it was set.
    pub flash_message: Option<(String, Instant)>,
}

impl SettingsState {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            draft: config.clone(),
            unsaved: false,
            selected: 0,
            editing: false,
            confirming_save: false,
            edit_buffer: String::new(),
            flash_message: None,
        }
    }

    /// The currently selected field.
    #[must_use]
    pub fn current_field(&self) -> SettingField {
        SettingField::ALL[self.selected]
    }

    /// Check if flash message has expired (>2s).
    pub fn expire_flash(&mut self) {
        if let Some((_, t)) = &self.flash_message {
            if t.elapsed().as_secs_f64() >= 2.0 {
                self.flash_message = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_visibility_is_independent_from_cost_calculation() {
        let mut config = Config::default();

        SettingField::ColCost.toggle_bool(&mut config);
        assert!(!config.columns.cost);
        assert!(!config.no_cost);

        SettingField::NoCost.toggle_bool(&mut config);
        assert!(config.no_cost);
        assert!(!config.columns.cost);
    }

    #[test]
    fn metric_fields_toggle_their_persisted_columns() {
        let mut config = Config::default();

        SettingField::ColRequests.toggle_bool(&mut config);
        SettingField::ColTotalTokens.toggle_bool(&mut config);
        SettingField::ColCost.toggle_bool(&mut config);

        assert!(!config.columns.requests);
        assert!(!config.columns.total_tokens);
        assert!(!config.columns.cost);
    }

    #[test]
    fn invalid_numeric_values_are_rejected_without_clamping() {
        let mut config = Config::default();
        assert!(SettingField::TickInterval
            .apply_value(&mut config, "301")
            .is_err());
        assert_eq!(config.tick_interval, 0);
        assert!(SettingField::TodayBucketMins
            .apply_value(&mut config, "0")
            .is_err());
        assert_eq!(config.today_bucket_mins, 10);
        assert!(SettingField::BudgetDaily
            .apply_value(&mut config, "-1")
            .is_err());
        assert!(config.budget.daily.is_none());
    }

    #[test]
    fn providers_are_editable_as_a_comma_separated_list() {
        let mut config = Config::default();
        SettingField::Providers
            .apply_value(&mut config, "alpha, beta")
            .expect("provider list should apply");
        assert_eq!(config.providers, ["alpha", "beta"]);
    }
}
