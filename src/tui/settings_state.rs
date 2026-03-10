use std::time::Instant;

use crate::config::Config;

// ── Settings state ────────────────────────────────────────────────────────

/// Which field is being displayed/edited in the settings view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingField {
    TickInterval,
    NoCost,
    DefaultCommand,
    SortOrder,
    ShowSparklines,
    SparklineMetric,
    TodayBucketMins,
    WeekBucketHours,
    MonthBucketDays,
    BudgetDaily,
    BudgetWeekly,
    BudgetMonthly,
    ColApiProvider,
    ColClient,
    ColInput,
    ColOutput,
}

impl SettingField {
    /// Total number of settings fields.
    pub const COUNT: usize = 16;

    /// All fields in display order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::TickInterval,
        Self::NoCost,
        Self::DefaultCommand,
        Self::SortOrder,
        Self::ShowSparklines,
        Self::SparklineMetric,
        Self::TodayBucketMins,
        Self::WeekBucketHours,
        Self::MonthBucketDays,
        Self::BudgetDaily,
        Self::BudgetWeekly,
        Self::BudgetMonthly,
        Self::ColApiProvider,
        Self::ColClient,
        Self::ColInput,
        Self::ColOutput,
    ];

    /// Display label for this field.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TickInterval => "Tick Interval (s) *",
            Self::NoCost => "Disable Costs",
            Self::DefaultCommand => "Default Command",
            Self::SortOrder => "Sort Order",
            Self::ShowSparklines => "Show Sparklines",
            Self::SparklineMetric => "Sparkline Metric",
            Self::TodayBucketMins => "Today Bar (mins)",
            Self::WeekBucketHours => "Week Bar (hours)",
            Self::MonthBucketDays => "Month Bar (days)",
            Self::BudgetDaily => "Daily Budget ($)",
            Self::BudgetWeekly => "Weekly Budget ($)",
            Self::BudgetMonthly => "Monthly Budget ($)",
            Self::ColApiProvider => "API Provider",
            Self::ColClient => "Client",
            Self::ColInput => "Input Tokens",
            Self::ColOutput => "Output Tokens",
        }
    }

    /// Section header for visual grouping (returns Some for the first item in each section).
    #[must_use]
    pub fn section_header(self) -> Option<&'static str> {
        match self {
            Self::TickInterval => Some("General"),
            Self::ShowSparklines => Some("Sparklines"),
            Self::BudgetDaily => Some("Budget Limits"),
            Self::ColApiProvider => Some("Columns"),
            _ => None,
        }
    }

    /// Whether this field is a boolean toggle.
    #[must_use]
    pub fn is_bool(self) -> bool {
        matches!(
            self,
            Self::NoCost
                | Self::ShowSparklines
                | Self::ColApiProvider
                | Self::ColClient
                | Self::ColInput
                | Self::ColOutput
        )
    }

    /// Whether this field is an enum that cycles through values.
    #[must_use]
    pub fn is_enum(self) -> bool {
        matches!(
            self,
            Self::DefaultCommand | Self::SortOrder | Self::SparklineMetric
        )
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
            Self::NoCost => if config.no_cost { "Yes" } else { "No" }.to_string(),
            Self::DefaultCommand => config.default_command.to_string(),
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
            Self::ColApiProvider => bool_display(config.columns.api_provider),
            Self::ColClient => bool_display(config.columns.client),
            Self::ColInput => bool_display(config.columns.input),
            Self::ColOutput => bool_display(config.columns.output),
        }
    }

    /// Toggle a boolean field on the given config. No-op for non-bool fields.
    pub fn toggle_bool(self, config: &mut Config) {
        match self {
            Self::NoCost => config.no_cost = !config.no_cost,
            Self::ShowSparklines => config.show_sparklines = !config.show_sparklines,
            Self::ColApiProvider => config.columns.api_provider = !config.columns.api_provider,
            Self::ColClient => config.columns.client = !config.columns.client,
            Self::ColInput => config.columns.input = !config.columns.input,
            Self::ColOutput => config.columns.output = !config.columns.output,
            _ => {}
        }
    }

    /// Cycle an enum field to its next value. No-op for non-enum fields.
    pub fn cycle_enum(self, config: &mut Config) {
        match self {
            Self::DefaultCommand => {
                config.default_command = config.default_command.next();
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
    /// Returns `true` if the value was valid and applied.
    pub fn apply_value(self, config: &mut Config, value: &str) -> bool {
        match self {
            Self::TickInterval => {
                if let Ok(v) = value.parse::<u64>() {
                    config.tick_interval = v.min(300);
                    true
                } else {
                    false
                }
            }
            Self::TodayBucketMins => {
                if let Ok(v) = value.parse::<u64>() {
                    config.today_bucket_mins = v.clamp(1, 60);
                    true
                } else {
                    false
                }
            }
            Self::WeekBucketHours => {
                if let Ok(v) = value.parse::<u64>() {
                    config.week_bucket_hours = v.clamp(1, 24);
                    true
                } else {
                    false
                }
            }
            Self::MonthBucketDays => {
                if let Ok(v) = value.parse::<u64>() {
                    config.month_bucket_days = v.clamp(1, 7);
                    true
                } else {
                    false
                }
            }
            Self::BudgetDaily => apply_budget_value(&mut config.budget.daily, value),
            Self::BudgetWeekly => apply_budget_value(&mut config.budget.weekly, value),
            Self::BudgetMonthly => apply_budget_value(&mut config.budget.monthly, value),
            _ => false,
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
            _ => String::new(),
        }
    }
}

pub(crate) fn bool_display(v: bool) -> String {
    if v { "Yes" } else { "No" }.to_string()
}

fn apply_budget_value(target: &mut Option<f64>, value: &str) -> bool {
    if value.is_empty() {
        *target = None;
        return true;
    }
    if let Ok(v) = value.parse::<f64>() {
        if v > 0.0 && v.is_finite() {
            *target = Some(v);
        } else {
            *target = None;
        }
        true
    } else {
        false
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
