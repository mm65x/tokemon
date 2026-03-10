use std::collections::HashMap;
use std::time::Instant;

use crate::timestamp;
use chrono::{Datelike, Duration, NaiveDate, Timelike, Utc};
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::Config;
use crate::render::{self, format_tokens_short};
use crate::source::SourceSet;
use crate::types::{DailySummary, ModelUsage, Record};
use crate::{cache, cost, dedup, rollup};

use super::diff::{self, RowKey};
use super::event::Event;

/// Duration (in seconds) for the per-cell highlight fade animation.
const HIGHLIGHT_DURATION_SECS: f64 = 1.5;

/// Duration (in seconds) for warnings to remain visible in the status bar.
const WARNING_DISPLAY_SECS: f64 = 5.0;

// ── View scope ────────────────────────────────────────────────────────────

/// Which time window the detail table shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Today,
    Week,
    Month,
    AllTime,
}

impl Scope {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Week => "This Week",
            Self::Month => "This Month",
            Self::AllTime => "All Time",
        }
    }

    /// Return the start date for this scope.
    #[must_use]
    pub fn since(self) -> NaiveDate {
        match self {
            Self::Today => timestamp::start_of_today(),
            Self::Week => timestamp::start_of_week(),
            Self::Month => timestamp::start_of_month(),
            Self::AllTime => NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
        }
    }
}

// ── Summary card data ─────────────────────────────────────────────────────

/// Data for one summary card (Today / This Week / This Month).
#[derive(Debug, Clone)]
pub struct CardData {
    pub label: &'static str,
    pub cost: f64,
    pub tokens: u64,
    pub sparkline: Vec<u64>,
    /// Trend indicator: positive = increasing, negative = decreasing, zero = flat.
    pub trend: i8,
}

impl CardData {
    #[must_use]
    pub fn cost_str(&self) -> String {
        render::format_cost(self.cost)
    }

    #[must_use]
    pub fn tokens_str(&self) -> String {
        format!("{} tokens", format_tokens_short(self.tokens))
    }

    /// Trend arrow for display.
    #[must_use]
    pub fn trend_symbol(&self) -> &'static str {
        match self.trend.cmp(&0) {
            std::cmp::Ordering::Greater => "↑",
            std::cmp::Ordering::Less => "↓",
            std::cmp::Ordering::Equal => "−",
        }
    }
}

// ── Group-by mode ─────────────────────────────────────────────────────────

/// How to group rows in the detail table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// One row per model (aggregated across all clients).
    Model,
    /// One row per model+client combination.
    ModelClient,
    /// One row per client (aggregated across all models).
    Client,
}

impl GroupBy {
    /// Cycle to the next group-by mode.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Model => Self::ModelClient,
            Self::ModelClient => Self::Client,
            Self::Client => Self::Model,
        }
    }

    /// Short label for display.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::ModelClient => "model+client",
            Self::Client => "client",
        }
    }
}

// ── Sort order ────────────────────────────────────────────────────────────

/// Sort order for the detail table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Sort by cost descending (default).
    CostDesc,
    /// Sort by total tokens descending.
    TokensDesc,
    /// Sort by model name ascending.
    NameAsc,
    /// Sort by request count descending.
    RequestsDesc,
}

impl SortOrder {
    /// Cycle to the next sort order.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::CostDesc => Self::TokensDesc,
            Self::TokensDesc => Self::NameAsc,
            Self::NameAsc => Self::RequestsDesc,
            Self::RequestsDesc => Self::CostDesc,
        }
    }

    /// Short label for display.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::CostDesc => "cost",
            Self::TokensDesc => "tokens",
            Self::NameAsc => "name",
            Self::RequestsDesc => "requests",
        }
    }
}

// ── Settings state ────────────────────────────────────────────────────────

/// Which field is being displayed/edited in the settings view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingField {
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
            Self::DefaultCommand => config.default_command.clone(),
            Self::SortOrder => config.sort_order.clone(),
            Self::ShowSparklines => if config.show_sparklines { "Yes" } else { "No" }.to_string(),
            Self::SparklineMetric => config.sparkline_metric.clone(),
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
                config.default_command = match config.default_command.as_str() {
                    "daily" => "weekly".to_string(),
                    "weekly" => "monthly".to_string(),
                    _ => "daily".to_string(),
                };
            }
            Self::SortOrder => {
                config.sort_order = match config.sort_order.as_str() {
                    "asc" => "desc".to_string(),
                    _ => "asc".to_string(),
                };
            }
            Self::SparklineMetric => {
                config.sparkline_metric = match config.sparkline_metric.as_str() {
                    "tokens" => "cost".to_string(),
                    _ => "tokens".to_string(),
                };
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

fn bool_display(v: bool) -> String {
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
pub struct SettingsState {
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
    fn new(config: &Config) -> Self {
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

// ── App state ─────────────────────────────────────────────────────────────

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    /// Currently selected detail scope.
    pub scope: Scope,
    /// How to group rows in the detail table.
    pub group_by: GroupBy,
    /// Whether history mode is toggled on.
    pub show_history: bool,
    /// Summary cards: Today, This Week, This Month, All Time.
    pub cards: [CardData; 4],
    /// Detail table rows for the selected scope.
    pub detail_models: Vec<ModelUsage>,
    /// Detail totals.
    pub detail_total_cost: f64,
    pub detail_total_tokens: u64,
    pub detail_total_requests: u64,
    /// Historical summaries (populated when `show_history` is true).
    pub history_summaries: Vec<DailySummary>,
    /// Scroll offset for the detail table.
    pub scroll_offset: u16,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Whether the help overlay is shown.
    pub show_help: bool,
    /// Whether the filter input is active.
    pub filter_active: bool,
    /// Current filter input text.
    pub filter_text: String,
    /// Applied filter (empty = no filter).
    pub applied_filter: String,
    /// Current sort order.
    pub sort_order: SortOrder,
    /// Per-row highlight timestamps: maps a `RowKey` to the instant
    /// it was last updated. Used for the green fade animation on
    /// individual table cells.
    pub highlight_map: HashMap<RowKey, Instant>,
    /// Last warning message from the background watcher or data loading,
    /// with the instant it was received. Displayed in the status bar
    /// for a few seconds then cleared.
    pub last_warning: Option<(String, Instant)>,
    /// Whether the settings overlay is shown.
    pub show_settings: bool,
    /// Settings editor state.
    pub settings_state: SettingsState,
    /// Whether the UI state has changed and needs a redraw.
    /// Set by event handlers, cleared after each frame is drawn.
    pub dirty: bool,
    /// Snapshot of config as passed to App (for settings editor).
    pub(crate) config: Config,
    /// Cached pricing engine (loaded once at startup, reused for all reads).
    pricing: Option<cost::PricingEngine>,
    /// Source registry (created once, reused for tick-based polling).
    registry: SourceSet,
    /// Whether to skip cost calculation.
    no_cost: bool,
    /// Cached raw records for the current data load.
    cached_records: Vec<Record>,
    /// Previous model snapshot for diffing.
    prev_models: Vec<ModelUsage>,
    /// Whether the initial data load is complete. The first load
    /// populates `prev_models` but does NOT trigger highlights,
    /// preventing the "everything flashes" effect on startup.
    initial_load_done: bool,
    /// Base all-time cost for records older than the in-memory window.
    /// Computed once at startup.
    all_time_base_cost: f64,
    /// Base all-time token count for records older than the in-memory window.
    all_time_base_tokens: u64,
    /// Weekly sparkline bars for historical records (before in-memory window).
    /// Each element is a token count for one ISO week, in chronological order.
    all_time_base_sparkline: Vec<u64>,
    /// ISO (year, week) of the first bar in `all_time_base_sparkline`.
    /// Used to align with current-window bars when merging.
    all_time_base_start_week: Option<(i32, u32)>,
    /// Aggregated model usage from historical records (before the in-memory
    /// window). Computed once at startup. When the user views the All Time
    /// scope, these are merged with current-window aggregations.
    all_time_base_models: Vec<ModelUsage>,
}

impl App {
    /// Create a new app and perform the initial data load.
    pub fn new(config: &Config, initial_scope: Scope) -> Self {
        let mut app = Self {
            scope: initial_scope,
            group_by: GroupBy::ModelClient,
            show_history: false,
            cards: [
                CardData {
                    label: "Today",
                    cost: 0.0,
                    tokens: 0,
                    sparkline: Vec::new(),
                    trend: 0,
                },
                CardData {
                    label: "This Week",
                    cost: 0.0,
                    tokens: 0,
                    sparkline: Vec::new(),
                    trend: 0,
                },
                CardData {
                    label: "This Month",
                    cost: 0.0,
                    tokens: 0,
                    sparkline: Vec::new(),
                    trend: 0,
                },
                CardData {
                    label: "All Time",
                    cost: 0.0,
                    tokens: 0,
                    sparkline: Vec::new(),
                    trend: 0,
                },
            ],
            detail_models: Vec::new(),
            detail_total_cost: 0.0,
            detail_total_tokens: 0,
            detail_total_requests: 0,
            history_summaries: Vec::new(),
            scroll_offset: 0,
            should_quit: false,
            show_help: false,
            filter_active: false,
            filter_text: String::new(),
            applied_filter: String::new(),
            sort_order: SortOrder::CostDesc,
            highlight_map: HashMap::new(),
            last_warning: None,
            show_settings: false,
            settings_state: SettingsState::new(config),
            dirty: true,
            config: config.clone(),
            pricing: None,
            registry: SourceSet::new(),
            no_cost: config.no_cost,
            cached_records: Vec::new(),
            prev_models: Vec::new(),
            initial_load_done: false,
            all_time_base_cost: 0.0,
            all_time_base_tokens: 0,
            all_time_base_sparkline: Vec::new(),
            all_time_base_start_week: None,
            all_time_base_models: Vec::new(),
        };
        // Load pricing engine once. Try offline first (fast path using
        // cached pricing.json). If the cache doesn't exist yet (fresh
        // install), fall back to a one-time online fetch.
        if !config.no_cost {
            app.pricing = cost::PricingEngine::load(true).ok();
            if app
                .pricing
                .as_ref()
                .is_some_and(cost::PricingEngine::is_empty)
            {
                app.pricing = cost::PricingEngine::load(false).ok();
            }
        }
        // Compute all-time base from historical records (before the
        // in-memory window). This runs once at startup.
        app.compute_all_time_base();
        // Initial data load: sync sources then read cache.
        app.poll_sources();
        app.reload_from_cache();
        app
    }

    /// Handle an incoming event. Returns `true` if the UI needs a redraw.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Key(key) => {
                let changed = self.handle_key(*key);
                self.dirty |= changed;
                changed
            }
            Event::Tick => {
                // Poll source files for changes (lightweight mtime checks),
                // re-parse any that changed, and update the cache.
                // The watcher thread also does this on file events, but
                // tick-based polling catches changes that `notify` may miss
                // (e.g. SQLite WAL writes on some platforms).
                self.poll_sources();
                self.dirty |= self.reload_from_cache();
                // Expire old warnings
                if let Some((_, t)) = &self.last_warning {
                    if t.elapsed().as_secs_f64() >= WARNING_DISPLAY_SECS {
                        self.last_warning = None;
                        self.dirty = true;
                    }
                }
                // Expire settings flash message
                if self.settings_state.flash_message.is_some() {
                    self.settings_state.expire_flash();
                    if self.settings_state.flash_message.is_none() {
                        self.dirty = true;
                    }
                }
                self.dirty
            }
            Event::DataChanged => {
                // The watcher already wrote to the cache — just re-read it.
                self.dirty |= self.reload_from_cache();
                self.dirty
            }
            Event::Warning(msg) => {
                self.last_warning = Some((msg.clone(), Instant::now()));
                self.dirty = true;
                true
            }
            Event::Resize(_, _) => {
                self.dirty = true;
                true
            }
            Event::Render => false,
        }
    }

    /// Returns the current warning message if it's still fresh (< 5 seconds old).
    #[must_use]
    pub fn active_warning(&self) -> Option<&str> {
        self.last_warning.as_ref().and_then(|(msg, t)| {
            if t.elapsed().as_secs_f64() < WARNING_DISPLAY_SECS {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    #[allow(clippy::too_many_lines)]
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Settings overlay takes priority
        if self.show_settings {
            return self.handle_settings_key(key);
        }

        // If help is shown, any key dismisses it
        if self.show_help {
            self.show_help = false;
            return true;
        }

        // Filter input mode
        if self.filter_active {
            return self.handle_filter_key(key);
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.applied_filter.is_empty() {
                    self.should_quit = true;
                    false
                } else {
                    // First Esc/q clears the filter
                    self.applied_filter.clear();
                    self.recompute_detail();
                    true
                }
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                true
            }
            KeyCode::Char('S') => {
                self.show_settings = true;
                self.settings_state = SettingsState::new(&self.config);
                true
            }
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter_text = self.applied_filter.clone();
                true
            }
            KeyCode::Char('t') => {
                self.scope = Scope::Today;
                self.reset_view_state();
                true
            }
            KeyCode::Char('w') => {
                self.scope = Scope::Week;
                self.reset_view_state();
                true
            }
            KeyCode::Char('m') => {
                self.scope = Scope::Month;
                self.reset_view_state();
                true
            }
            KeyCode::Char('a') => {
                self.scope = Scope::AllTime;
                self.reset_view_state();
                true
            }
            KeyCode::Char('s') => {
                self.sort_order = self.sort_order.next();
                self.reset_view_state();
                true
            }
            KeyCode::Char('g') => {
                self.group_by = self.group_by.next();
                self.compute_all_time_base();
                self.prev_models.clear();
                self.highlight_map.clear();
                self.recompute_detail();
                true
            }
            KeyCode::Char('h') => {
                self.show_history = !self.show_history;
                self.recompute_detail();
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let max = self.detail_models.len().saturating_sub(1) as u16;
                self.scroll_offset = self.scroll_offset.saturating_add(1).min(max);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                true
            }
            KeyCode::Left => {
                let new_scope = match self.scope {
                    Scope::Today | Scope::Week => Scope::Today,
                    Scope::Month => Scope::Week,
                    Scope::AllTime => Scope::Month,
                };
                if new_scope == self.scope {
                    false
                } else {
                    self.scope = new_scope;
                    self.reset_view_state();
                    true
                }
            }
            KeyCode::Right => {
                let new_scope = match self.scope {
                    Scope::Today => Scope::Week,
                    Scope::Week => Scope::Month,
                    Scope::Month | Scope::AllTime => Scope::AllTime,
                };
                if new_scope == self.scope {
                    false
                } else {
                    self.scope = new_scope;
                    self.reset_view_state();
                    true
                }
            }
            _ => false,
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                self.filter_active = false;
                self.applied_filter = self.filter_text.clone();
                self.reset_view_state();
                true
            }
            KeyCode::Esc => {
                self.filter_active = false;
                self.filter_text.clear();
                true
            }
            KeyCode::Char(c) => {
                self.filter_text.push(c);
                true
            }
            KeyCode::Backspace => {
                self.filter_text.pop();
                true
            }
            _ => false,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_settings_key(&mut self, key: KeyEvent) -> bool {
        let state = &mut self.settings_state;

        // If editing a text/numeric field
        if state.editing {
            match key.code {
                KeyCode::Enter => {
                    let field = state.current_field();
                    if field.apply_value(&mut state.draft, &state.edit_buffer) {
                        state.unsaved = true;
                    }
                    state.editing = false;
                    state.edit_buffer.clear();
                    return true;
                }
                KeyCode::Esc => {
                    state.editing = false;
                    state.edit_buffer.clear();
                    return true;
                }
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                    state.edit_buffer.push(c);
                    return true;
                }
                KeyCode::Backspace => {
                    state.edit_buffer.pop();
                    return true;
                }
                _ => return false,
            }
        }

        // Normal settings navigation
        match key.code {
            KeyCode::Esc | KeyCode::Char('S') => {
                // Close settings, discard unsaved changes
                self.show_settings = false;
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                state.selected = (state.selected + 1).min(SettingField::COUNT - 1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                state.selected = state.selected.saturating_sub(1);
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let field = state.current_field();
                if field.is_bool() {
                    field.toggle_bool(&mut state.draft);
                    state.unsaved = true;
                    true
                } else if field.is_enum() {
                    field.cycle_enum(&mut state.draft);
                    state.unsaved = true;
                    true
                } else {
                    // Enter edit mode for numeric fields
                    state.editing = true;
                    state.edit_buffer = field.edit_value(&state.draft);
                    true
                }
            }
            KeyCode::Left => {
                let field = state.current_field();
                if field.is_enum() {
                    // Cycle backwards — just cycle forward (there are only 2-3 values)
                    field.cycle_enum(&mut state.draft);
                    state.unsaved = true;
                    true
                } else {
                    false
                }
            }
            KeyCode::Right => {
                let field = state.current_field();
                if field.is_enum() {
                    field.cycle_enum(&mut state.draft);
                    state.unsaved = true;
                    true
                } else {
                    false
                }
            }
            KeyCode::Char('W') => {
                // Save to disk
                let old_metric = self.config.sparkline_metric.clone();
                match self.settings_state.draft.save() {
                    Ok(()) => {
                        self.config = self.settings_state.draft.clone();
                        self.no_cost = self.settings_state.draft.no_cost;
                        self.settings_state.unsaved = false;
                        self.settings_state.flash_message =
                            Some(("Saved!".to_string(), Instant::now()));
                        // Recompute all-time base if sparkline metric changed
                        if self.config.sparkline_metric != old_metric {
                            self.compute_all_time_base();
                        }
                    }
                    Err(e) => {
                        self.last_warning =
                            Some((format!("Failed to save config: {e}"), Instant::now()));
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Load all records older than the in-memory window, apply pricing,
    /// and compute base totals and weekly sparkline for the All Time card.
    /// Called once at startup.
    fn compute_all_time_base(&mut self) {
        let cutoff = Scope::Month.since() - Duration::days(30);
        let Some(cutoff_pred) = cutoff.pred_opt() else {
            return;
        };

        let Ok(cache) = cache::Cache::open() else {
            return;
        };
        let mut historical = cache
            .load_entries_filtered(None, Some(cutoff_pred), &[])
            .unwrap_or_default();

        if historical.is_empty() {
            return;
        }

        // Apply pricing to historical records.
        if let Some(engine) = self.pricing.as_ref() {
            engine.apply_costs(&mut historical);
        }

        self.all_time_base_cost = historical.iter().map(|r| r.cost_usd.unwrap_or(0.0)).sum();
        self.all_time_base_tokens = historical
            .iter()
            .map(super::super::types::Record::total_tokens)
            .sum();

        // Build weekly sparkline from historical records.
        let use_cost = self.config.sparkline_metric == "cost";
        let (sparkline, start_week) = build_weekly_sparkline_data(&historical, use_cost);
        self.all_time_base_sparkline = sparkline;
        self.all_time_base_start_week = start_week;

        // Aggregate into model-level breakdown for the detail table.
        let summaries = rollup::aggregate_daily(&historical);
        self.all_time_base_models = aggregate_summaries_to_models(&summaries, self.group_by);
    }

    /// Poll source files for mtime changes, re-parse any that changed,
    /// and write the results to the cache in a single transaction.
    ///
    /// This is the primary mechanism for detecting live data updates.
    /// It checks file modification times (including SQLite WAL siblings)
    /// against the mtimes stored in the cache. Only files with newer
    /// mtimes are re-parsed, so the cost is negligible when nothing changed.
    fn poll_sources(&self) {
        let Ok(mut cache) = cache::Cache::open() else {
            return;
        };
        let cached_mtimes = cache.cached_file_mtimes().unwrap_or_default();
        let providers = self.registry.available();

        // Collect parsed results with owned PathBufs.
        let mut parsed: Vec<(std::path::PathBuf, i64, Vec<crate::types::Record>)> = Vec::new();

        for provider in &providers {
            for file in provider.discover_files() {
                let mtime = cache::file_mtime_secs_for_db(&file).unwrap_or(0);
                let file_key = file.display().to_string();
                if cached_mtimes.get(&file_key) == Some(&mtime) {
                    continue;
                }
                if let Ok(entries) = provider.parse_file(&file) {
                    // Don't apply pricing here — the cache stores raw source
                    // data. Pricing is applied at read time in
                    // load_records_from_cache(), matching the CLI path.
                    let entries = dedup::deduplicate(entries);
                    parsed.push((file, mtime, entries));
                }
            }
        }

        if parsed.is_empty() {
            return;
        }

        // write_entries takes &[(&Path, i64, Vec<Record>)].
        // We own PathBufs in `parsed` — build refs that borrow from them.
        let refs: Vec<(&std::path::Path, i64, Vec<crate::types::Record>)> = parsed
            .iter_mut()
            .map(|(p, m, e)| (p.as_path(), *m, std::mem::take(e)))
            .collect();

        let _ = cache.write_entries(&refs);
    }

    /// Re-read the cache and recompute all derived state.
    /// Returns `true` if data actually changed (new highlights or initial load).
    ///
    /// This is lightweight — it only queries SQLite and recomputes
    /// in-memory aggregations. File discovery and parsing are handled
    /// by the background watcher thread.
    fn reload_from_cache(&mut self) -> bool {
        let records = load_records_from_cache(self.pricing.as_ref());
        self.cached_records = records;

        // Snapshot cards before recomputing to detect card-only changes.
        let prev_cards_tokens: Vec<u64> = self.cards.iter().map(|c| c.tokens).collect();
        #[allow(clippy::cast_possible_truncation)]
        let prev_cards_cost: Vec<i64> = self
            .cards
            .iter()
            .map(|c| (c.cost * 10_000.0) as i64)
            .collect();
        let prev_sparklines: Vec<Vec<u64>> =
            self.cards.iter().map(|c| c.sparkline.clone()).collect();

        self.recompute_cards();

        // Check if any card data changed (cost, tokens, or sparkline).
        #[allow(clippy::cast_possible_truncation)]
        let cards_dirty = self.cards.iter().enumerate().any(|(i, c)| {
            c.tokens != prev_cards_tokens[i]
                || (c.cost * 10_000.0) as i64 != prev_cards_cost[i]
                || c.sparkline != prev_sparklines[i]
        });

        // Snapshot current models before recomputing for diff
        let prev = std::mem::take(&mut self.prev_models);
        self.recompute_detail();

        // On first load, just seed prev_models — no highlights.
        if !self.initial_load_done {
            self.initial_load_done = true;
            self.prev_models = self.detail_models.clone();
            return true;
        }

        // Compute diff against previous state
        let changes = diff::diff(&prev, &self.detail_models);
        let has_changes = !changes.is_empty();
        let now = Instant::now();
        for change in &changes {
            self.highlight_map.insert(change.key.clone(), now);
        }

        // Expire old highlights
        self.highlight_map
            .retain(|_, t| t.elapsed().as_secs_f64() < HIGHLIGHT_DURATION_SECS);

        // Save current models for next diff
        self.prev_models = self.detail_models.clone();

        has_changes || cards_dirty
    }

    /// Returns `true` if any per-row highlights are still actively fading.
    /// Used by the main loop to keep redrawing during animations.
    #[must_use]
    pub fn has_active_highlights(&self) -> bool {
        self.highlight_map
            .values()
            .any(|t| t.elapsed().as_secs_f64() < HIGHLIGHT_DURATION_SECS)
    }

    /// Return the highlight intensity (0.0–1.0) for a given row key.
    /// Returns 0.0 if the row has no active highlight.
    #[must_use]
    pub fn highlight_intensity(&self, key: &RowKey) -> f64 {
        self.highlight_map.get(key).map_or(0.0, |t| {
            let elapsed = t.elapsed().as_secs_f64();
            if elapsed >= HIGHLIGHT_DURATION_SECS {
                0.0
            } else {
                1.0 - (elapsed / HIGHLIGHT_DURATION_SECS)
            }
        })
    }

    fn recompute_cards(&mut self) {
        let today = Utc::now().date_naive();
        let use_cost = self.config.sparkline_metric == "cost";

        // Today card
        let today_records: Vec<&Record> = self
            .cached_records
            .iter()
            .filter(|r| r.timestamp.date_naive() == today)
            .collect();
        self.cards[0].cost = sum_cost(&today_records);
        self.cards[0].tokens = sum_tokens(&today_records);

        // This week card
        let week_start = Scope::Week.since();
        let week_records: Vec<&Record> = self
            .cached_records
            .iter()
            .filter(|r| r.timestamp.date_naive() >= week_start)
            .collect();
        self.cards[1].cost = sum_cost(&week_records);
        self.cards[1].tokens = sum_tokens(&week_records);

        // Build sparkline: N-hour buckets for the week
        #[allow(clippy::cast_possible_truncation)]
        let week_bucket_hours = self.config.week_bucket_hours as u32;
        self.cards[1].sparkline = build_hour_sparkline(
            &self.cached_records,
            week_start,
            week_bucket_hours,
            use_cost,
        );

        // This month card
        let month_start = Scope::Month.since();
        let month_records: Vec<&Record> = self
            .cached_records
            .iter()
            .filter(|r| r.timestamp.date_naive() >= month_start)
            .collect();
        self.cards[2].cost = sum_cost(&month_records);
        self.cards[2].tokens = sum_tokens(&month_records);

        // Build sparkline: N-day buckets for the month
        #[allow(clippy::cast_possible_truncation)]
        let month_bucket_days = self.config.month_bucket_days as u32;
        self.cards[2].sparkline = build_day_sparkline(
            &self.cached_records,
            month_start,
            month_bucket_days,
            use_cost,
        );

        // Today sparkline: N-minute buckets
        #[allow(clippy::cast_possible_truncation)]
        let today_bucket_mins = self.config.today_bucket_mins as u32;
        self.cards[0].sparkline =
            build_minute_sparkline(&self.cached_records, today_bucket_mins, use_cost);

        // All Time card: base (historical) + current window
        let window_cost: f64 = self
            .cached_records
            .iter()
            .map(|r| r.cost_usd.unwrap_or(0.0))
            .sum();
        let window_tokens: u64 = self
            .cached_records
            .iter()
            .map(super::super::types::Record::total_tokens)
            .sum();
        self.cards[3].cost = self.all_time_base_cost + window_cost;
        self.cards[3].tokens = self.all_time_base_tokens + window_tokens;
        self.cards[3].sparkline = merge_weekly_sparklines(
            &self.all_time_base_sparkline,
            self.all_time_base_start_week,
            &self.cached_records,
            use_cost,
        );

        // Compute trends from sparkline data
        for card in &mut self.cards {
            card.trend = compute_trend(&card.sparkline);
        }
    }

    #[allow(clippy::too_many_lines)]
    /// Reset view state when scope/filter/sort/group-by changes.
    /// Clears highlights and scroll, then recomputes the detail table.
    fn reset_view_state(&mut self) {
        self.scroll_offset = 0;
        self.prev_models.clear();
        self.highlight_map.clear();
        self.recompute_detail();
    }

    fn recompute_detail(&mut self) {
        let since = self.scope.since();
        let filtered: Vec<Record> = self
            .cached_records
            .iter()
            .filter(|r| r.timestamp.date_naive() >= since)
            .cloned()
            .collect();

        // Aggregate into model-level breakdown for the selected scope
        let summaries = rollup::aggregate_daily(&filtered);
        let window_models = aggregate_summaries_to_models(&summaries, self.group_by);

        // For All Time, merge historical base models with current window.
        let mut models = if self.scope == Scope::AllTime {
            merge_model_usages(&self.all_time_base_models, &window_models)
        } else {
            window_models
        };

        // Apply provider/model filter if set
        if !self.applied_filter.is_empty() {
            let filter_lower = self.applied_filter.to_lowercase();
            models.retain(|m| {
                m.model.to_lowercase().contains(&filter_lower)
                    || m.provider.to_lowercase().contains(&filter_lower)
                    || crate::display::infer_api_provider(&m.model)
                        .to_lowercase()
                        .contains(&filter_lower)
            });
        }

        // Apply current sort order (stable sort to prevent shuffling of equal rows)
        // Always use model name as tiebreaker for deterministic ordering.
        match self.sort_order {
            SortOrder::CostDesc => {
                models.sort_by(|a, b| {
                    b.cost_usd
                        .total_cmp(&a.cost_usd)
                        .then_with(|| a.model.cmp(&b.model))
                });
            }
            SortOrder::TokensDesc => {
                models.sort_by(|a, b| {
                    let ta = a.total_tokens();
                    let tb = b.total_tokens();
                    tb.cmp(&ta).then_with(|| a.model.cmp(&b.model))
                });
            }
            SortOrder::NameAsc => {
                models.sort_by(|a, b| a.model.cmp(&b.model));
            }
            SortOrder::RequestsDesc => {
                models.sort_by(|a, b| {
                    b.request_count
                        .cmp(&a.request_count)
                        .then_with(|| a.model.cmp(&b.model))
                });
            }
        }

        self.detail_total_cost = models.iter().map(|m| m.cost_usd).sum();
        self.detail_total_tokens = models
            .iter()
            .map(super::super::types::ModelUsage::total_tokens)
            .sum();
        self.detail_total_requests = models.iter().map(|m| m.request_count).sum();
        self.detail_models = models;

        // Historical summaries for the history view
        if self.show_history {
            self.history_summaries = match self.scope {
                Scope::Today | Scope::Week => rollup::aggregate_daily(&filtered),
                Scope::Month | Scope::AllTime => rollup::aggregate_weekly(&filtered),
            };
        } else {
            self.history_summaries.clear();
        }
    }
}

/// Aggregate `DailySummary` model usages into a flat `Vec<ModelUsage>`
/// grouped by the selected `GroupBy` mode. Used by both `recompute_detail`
/// and `compute_all_time_base`.
fn aggregate_summaries_to_models(summaries: &[DailySummary], group_by: GroupBy) -> Vec<ModelUsage> {
    let mut model_map: std::collections::HashMap<(String, String), ModelUsage> =
        std::collections::HashMap::new();

    for summary in summaries {
        for mu in &summary.models {
            let key = match group_by {
                GroupBy::Model => (mu.model.clone(), String::new()),
                GroupBy::ModelClient => (mu.model.clone(), mu.provider.clone()),
                GroupBy::Client => (String::new(), mu.provider.clone()),
            };
            let entry = model_map.entry(key).or_insert_with(|| match group_by {
                GroupBy::Model => ModelUsage {
                    model: mu.model.clone(),
                    raw_model: mu.model.clone(),
                    provider: String::new(),
                    ..Default::default()
                },
                GroupBy::ModelClient => ModelUsage {
                    model: mu.model.clone(),
                    raw_model: mu.raw_model.clone(),
                    provider: mu.provider.clone(),
                    ..Default::default()
                },
                GroupBy::Client => ModelUsage {
                    model: String::new(),
                    raw_model: String::new(),
                    provider: mu.provider.clone(),
                    ..Default::default()
                },
            });
            entry.input_tokens += mu.input_tokens;
            entry.output_tokens += mu.output_tokens;
            entry.cache_read_tokens += mu.cache_read_tokens;
            entry.cache_creation_tokens += mu.cache_creation_tokens;
            entry.thinking_tokens += mu.thinking_tokens;
            entry.cost_usd += mu.cost_usd;
            entry.request_count += mu.request_count;
        }
    }

    model_map.into_values().collect()
}

/// Merge two sets of `ModelUsage` by summing values for matching keys.
fn merge_model_usages(base: &[ModelUsage], window: &[ModelUsage]) -> Vec<ModelUsage> {
    let mut map: std::collections::HashMap<(String, String), ModelUsage> =
        std::collections::HashMap::new();

    for mu in base.iter().chain(window.iter()) {
        let key = (mu.model.clone(), mu.provider.clone());
        let entry = map.entry(key).or_insert_with(|| ModelUsage {
            model: mu.model.clone(),
            raw_model: mu.raw_model.clone(),
            provider: mu.provider.clone(),
            ..Default::default()
        });
        entry.input_tokens += mu.input_tokens;
        entry.output_tokens += mu.output_tokens;
        entry.cache_read_tokens += mu.cache_read_tokens;
        entry.cache_creation_tokens += mu.cache_creation_tokens;
        entry.thinking_tokens += mu.thinking_tokens;
        entry.cost_usd += mu.cost_usd;
        entry.request_count += mu.request_count;
    }

    map.into_values().collect()
}

// ── Data loading ──────────────────────────────────────────────────────────

fn load_records_from_cache(pricing: Option<&cost::PricingEngine>) -> Vec<Record> {
    let Ok(c) = cache::Cache::open() else {
        return Vec::new();
    };

    // Load everything — the TUI filters in memory for card summaries.
    // We load the last ~60 days to keep things bounded.
    let since = Scope::Month.since() - chrono::Duration::days(30);
    let mut entries = c
        .load_entries_filtered(Some(since), None, &[])
        .unwrap_or_default();

    // Apply pricing from pre-loaded engine (no disk I/O here).
    if let Some(engine) = pricing {
        engine.apply_costs(&mut entries);
    }

    // Dedup is handled inside load_entries_filtered.
    entries.sort_by_key(|e| e.timestamp);
    entries
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn sum_cost(records: &[&Record]) -> f64 {
    records.iter().map(|r| r.cost_usd.unwrap_or(0.0)).sum()
}

fn sum_tokens(records: &[&Record]) -> u64 {
    records.iter().map(|r| r.total_tokens()).sum()
}

/// Extract the sparkline metric value from a record.
/// For "cost" mode, scales USD to millicents (x100_000) so small values are visible.
fn sparkline_value(record: &Record, use_cost: bool) -> u64 {
    if use_cost {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (record.cost_usd.unwrap_or(0.0) * 100_000.0) as u64;
        v
    } else {
        record.total_tokens()
    }
}

/// Build a sparkline of 10-minute buckets for today.
/// Produces one bucket per 10-minute slot from midnight to the current time.
/// Build a sparkline of N-minute buckets for today.
/// `bucket_mins` controls granularity (e.g. 1, 5, 10, 30, 60).
fn build_minute_sparkline(records: &[Record], bucket_mins: u32, use_cost: bool) -> Vec<u64> {
    let bucket_mins = bucket_mins.max(1);
    let now = Utc::now();
    let today = now.date_naive();
    let total_minutes = now.hour() * 60 + now.minute();
    let current_slot = (total_minutes / bucket_mins) as usize;
    let num_slots = current_slot + 1;
    let mut data = vec![0u64; num_slots];

    for record in records {
        if record.timestamp.date_naive() == today {
            let rm = record.timestamp.hour() * 60 + record.timestamp.minute();
            let slot = (rm / bucket_mins) as usize;
            if slot < num_slots {
                data[slot] += sparkline_value(record, use_cost);
            }
        }
    }

    data
}

/// Build a sparkline of N-hour buckets since `since_date`.
/// `bucket_hours` controls granularity (e.g. 1, 2, 4, 6, 12, 24).
fn build_hour_sparkline(
    records: &[Record],
    since_date: NaiveDate,
    bucket_hours: u32,
    use_cost: bool,
) -> Vec<u64> {
    let bucket_hours = bucket_hours.max(1);
    let slots_per_day = 24_u32.div_ceil(bucket_hours); // ceil(24/bucket_hours)
    let now = Utc::now();
    let today = now.date_naive();
    let total_days = (today - since_date).num_days().max(0) as usize + 1;
    let current_slot = (now.hour() / bucket_hours) as usize;
    let num_slots = if total_days > 1 {
        (total_days - 1) * slots_per_day as usize + current_slot + 1
    } else {
        current_slot + 1
    };
    let mut data = vec![0u64; num_slots];

    for record in records {
        let rd = record.timestamp.date_naive();
        if rd < since_date {
            continue;
        }
        let day_offset = (rd - since_date).num_days() as usize;
        let slot_in_day = (record.timestamp.hour() / bucket_hours) as usize;
        let idx = day_offset * slots_per_day as usize + slot_in_day;
        if idx < num_slots {
            data[idx] += sparkline_value(record, use_cost);
        }
    }

    data
}

/// Build a sparkline of N-day buckets since `since_date`.
/// `bucket_days` controls granularity (e.g. 1, 2, 7).
fn build_day_sparkline(
    records: &[Record],
    since_date: NaiveDate,
    bucket_days: u32,
    use_cost: bool,
) -> Vec<u64> {
    let bucket_days = i64::from(bucket_days.max(1));
    let today = Utc::now().date_naive();
    let total_days = (today - since_date).num_days().max(0) + 1;
    let num_slots = ((total_days + bucket_days - 1) / bucket_days) as usize; // ceil
    let mut data = vec![0u64; num_slots];

    for record in records {
        let rd = record.timestamp.date_naive();
        if rd < since_date {
            continue;
        }
        let day_offset = (rd - since_date).num_days();
        let idx = (day_offset / bucket_days) as usize;
        if idx < num_slots {
            data[idx] += sparkline_value(record, use_cost);
        }
    }

    data
}

/// Build weekly sparkline data from a set of records.
/// Returns `(sparkline_vec, start_week)` where `start_week` is `Some((iso_year, iso_week))`
/// of the first bar, or `None` if no records.
fn build_weekly_sparkline_data(
    records: &[Record],
    use_cost: bool,
) -> (Vec<u64>, Option<(i32, u32)>) {
    if records.is_empty() {
        return (Vec::new(), None);
    }

    // Find the range of ISO weeks
    let first_week = records
        .iter()
        .map(|r| r.timestamp.date_naive().iso_week())
        .min()
        .unwrap();
    let last_week = records
        .iter()
        .map(|r| r.timestamp.date_naive().iso_week())
        .max()
        .unwrap();

    let start_year = records
        .iter()
        .map(|r| r.timestamp.date_naive().iso_week())
        .min()
        .map(|_| {
            records
                .iter()
                .filter(|r| r.timestamp.date_naive().iso_week() == first_week)
                .map(|r| r.timestamp.date_naive())
                .min()
                .unwrap()
        })
        .unwrap();

    let start_yw = (start_year.iso_week().year(), start_year.iso_week().week());

    // Calculate total weeks span
    let end_date = records
        .iter()
        .filter(|r| r.timestamp.date_naive().iso_week() == last_week)
        .map(|r| r.timestamp.date_naive())
        .max()
        .unwrap();
    let end_yw = (end_date.iso_week().year(), end_date.iso_week().week());

    let total_weeks = iso_week_diff(start_yw, end_yw) + 1;
    let mut data = vec![0u64; total_weeks];

    for record in records {
        let rd = record.timestamp.date_naive();
        let yw = (rd.iso_week().year(), rd.iso_week().week());
        let idx = iso_week_diff(start_yw, yw);
        if idx < total_weeks {
            data[idx] += sparkline_value(record, use_cost);
        }
    }

    (data, Some(start_yw))
}

/// Compute the number of ISO weeks between two (year, week) pairs.
fn iso_week_diff(start: (i32, u32), end: (i32, u32)) -> usize {
    // Use NaiveDate to compute the difference in days, then divide by 7.
    // Monday of each ISO week.
    let start_date = NaiveDate::from_isoywd_opt(start.0, start.1, chrono::Weekday::Mon)
        .unwrap_or(NaiveDate::from_ymd_opt(start.0, 1, 1).unwrap());
    let end_date = NaiveDate::from_isoywd_opt(end.0, end.1, chrono::Weekday::Mon)
        .unwrap_or(NaiveDate::from_ymd_opt(end.0, 1, 1).unwrap());
    let days = (end_date - start_date).num_days().max(0);
    (days / 7) as usize
}

/// Merge the historical base weekly sparkline with current-window records
/// into a single weekly sparkline for the All Time card.
fn merge_weekly_sparklines(
    base: &[u64],
    base_start: Option<(i32, u32)>,
    current_records: &[Record],
    use_cost: bool,
) -> Vec<u64> {
    let now = Utc::now().date_naive();
    let now_yw = (now.iso_week().year(), now.iso_week().week());

    if base.is_empty() && current_records.is_empty() {
        return Vec::new();
    }

    // If no base, just build from current records
    if base.is_empty() || base_start.is_none() {
        let (sparkline, _) = build_weekly_sparkline_data(current_records, use_cost);
        return sparkline;
    }

    let start_yw = base_start.unwrap();
    let total_weeks = iso_week_diff(start_yw, now_yw) + 1;
    let mut data = vec![0u64; total_weeks];

    // Copy base data
    for (i, &val) in base.iter().enumerate() {
        if i < total_weeks {
            data[i] = val;
        }
    }

    // Add current window records
    for record in current_records {
        let rd = record.timestamp.date_naive();
        let yw = (rd.iso_week().year(), rd.iso_week().week());
        let idx = iso_week_diff(start_yw, yw);
        if idx < total_weeks {
            data[idx] += sparkline_value(record, use_cost);
        }
    }

    data
}

/// Compute a simple trend from sparkline data.
/// Compares the last value to the average of previous values.
fn compute_trend(data: &[u64]) -> i8 {
    if data.len() < 2 {
        return 0;
    }
    let last = data[data.len() - 1];
    #[allow(clippy::cast_possible_truncation)]
    let prev_avg = data[..data.len() - 1].iter().sum::<u64>() / (data.len() as u64 - 1).max(1);
    if last > prev_avg.saturating_add(prev_avg / 10) {
        1 // increasing
    } else if last < prev_avg.saturating_sub(prev_avg / 10) {
        -1 // decreasing
    } else {
        0 // flat
    }
}
