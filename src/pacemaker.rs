use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::config::{BudgetConfig, TimezoneMode};
use crate::render::format_tokens_short;
use crate::timestamp;
use crate::types::Record;

/// Rolling window (in minutes) used for live burn-rate calculations.
pub const BURN_RATE_WINDOW_MINUTES: i64 = 60;

/// Rolling-window spending and token velocity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[must_use]
pub struct BurnRate {
    pub tokens_per_min: f64,
    pub cost_per_hour: f64,
    pub window_tokens: u64,
    pub window_cost: f64,
}

impl BurnRate {
    /// Returns true when there is nonzero activity in the rolling window.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.window_tokens > 0 || self.window_cost > 0.0
    }
}

/// Compute the live burn rate over the default rolling 60-minute window ending now.
pub fn compute_burn_rate(entries: &[Record]) -> BurnRate {
    compute_burn_rate_at(
        entries,
        Utc::now(),
        Duration::minutes(BURN_RATE_WINDOW_MINUTES),
    )
}

/// Compute the burn rate over `window` ending at `now`.
pub fn compute_burn_rate_at(entries: &[Record], now: DateTime<Utc>, window: Duration) -> BurnRate {
    let secs = window.num_seconds();
    if secs <= 0 {
        return BurnRate::default();
    }
    let start = now - window;
    let mut window_tokens = 0u64;
    let mut window_cost = 0.0f64;

    for r in entries {
        if r.timestamp >= start && r.timestamp <= now {
            window_tokens = window_tokens.saturating_add(r.total_tokens());
            window_cost += r.cost_usd.unwrap_or(0.0);
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let hours = secs as f64 / 3600.0;
    #[allow(clippy::cast_precision_loss)]
    let mins = secs as f64 / 60.0;
    #[allow(clippy::cast_precision_loss)]
    let tokens_per_min = window_tokens as f64 / mins;
    let cost_per_hour = window_cost / hours;

    BurnRate {
        tokens_per_min,
        cost_per_hour,
        window_tokens,
        window_cost,
    }
}

/// Format a [`BurnRate`] concisely for headers and status lines (e.g. `$1.42/hr · 14.5k/m`).
#[must_use]
pub fn format_burn_rate(rate: &BurnRate) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rounded_tpm = rate.tokens_per_min.max(0.0).round() as u64;
    format!(
        "${:.2}/hr · {}/m",
        rate.cost_per_hour,
        format_tokens_short(rounded_tpm)
    )
}

/// Spending versus limit for a single budget period.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct BudgetPeriod {
    pub spent: f64,
    pub limit: f64,
}

/// Spending status across all configured budget periods.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct BudgetStatus {
    pub daily: Option<BudgetPeriod>,
    pub weekly: Option<BudgetPeriod>,
    pub monthly: Option<BudgetPeriod>,
}

/// Evaluate spending against budget limits in UTC.
pub fn evaluate(entries: &[Record], budget: &BudgetConfig) -> BudgetStatus {
    evaluate_in(entries, budget, TimezoneMode::Utc)
}

/// Evaluate spending against budget limits in `tz`.
pub fn evaluate_in(entries: &[Record], budget: &BudgetConfig, tz: TimezoneMode) -> BudgetStatus {
    let daily = budget.daily.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::today_in(tz), tz);
        BudgetPeriod { spent, limit }
    });

    let weekly = budget.weekly.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_week_in(tz), tz);
        BudgetPeriod { spent, limit }
    });

    let monthly = budget.monthly.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_month_in(tz), tz);
        BudgetPeriod { spent, limit }
    });

    BudgetStatus {
        daily,
        weekly,
        monthly,
    }
}

fn sum_cost_since(entries: &[Record], since: NaiveDate, tz: TimezoneMode) -> f64 {
    entries
        .iter()
        .filter(|e| timestamp::date_for(e.timestamp, tz) >= since)
        .filter_map(|e| e.cost_usd)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Record;
    use chrono::{Duration, Utc};

    fn create_dummy_record(cost: f64, offset_days: i64) -> Record {
        Record {
            timestamp: Utc::now() + Duration::days(offset_days),
            provider: "test".into(),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            thinking_tokens: 0,
            cost_usd: Some(cost),
            message_id: None,
            request_id: None,
            session_id: None,
        }
    }

    #[test]
    fn test_evaluate_no_budgets() {
        let entries = vec![create_dummy_record(1.5, 0)];
        let budget = BudgetConfig::default();
        let status = evaluate(&entries, &budget);

        assert!(status.daily.is_none());
        assert!(status.weekly.is_none());
        assert!(status.monthly.is_none());
    }

    #[test]
    fn test_evaluate_daily_budget() {
        let entries = vec![
            create_dummy_record(2.0, 0),
            create_dummy_record(3.5, 0),
            create_dummy_record(1.0, -2), // 2 days ago
        ];
        let budget = BudgetConfig {
            daily: Some(5.0),
            ..Default::default()
        };
        let status = evaluate(&entries, &budget);

        let daily = status.daily.expect("Daily budget should be evaluated");
        assert_eq!(daily.limit, 5.0);
        // Both today records should sum to 5.5
        assert_eq!(daily.spent, 5.5);
    }

    #[test]
    fn test_evaluate_all_budgets() {
        // Records placed today to guarantee they fall into current week/month
        let entries = vec![create_dummy_record(10.0, 0), create_dummy_record(5.0, 0)];
        let budget = BudgetConfig {
            daily: Some(20.0),
            weekly: Some(100.0),
            monthly: Some(500.0),
        };
        let status = evaluate(&entries, &budget);

        assert_eq!(status.daily.unwrap().spent, 15.0);
        assert_eq!(status.weekly.unwrap().spent, 15.0);
        assert_eq!(status.monthly.unwrap().spent, 15.0);

        assert_eq!(status.daily.unwrap().limit, 20.0);
        assert_eq!(status.weekly.unwrap().limit, 100.0);
        assert_eq!(status.monthly.unwrap().limit, 500.0);
    }

    #[test]
    fn test_sum_cost_since() {
        let entries = vec![create_dummy_record(1.0, 0), create_dummy_record(2.0, 0)];
        let since_today = timestamp::start_of_today();
        assert_eq!(
            sum_cost_since(&entries, since_today, TimezoneMode::Utc),
            3.0
        );
    }

    #[test]
    fn test_compute_burn_rate_filters_window_and_formats() {
        let now = Utc::now();
        let mut inside = create_dummy_record(2.5, 0);
        inside.timestamp = now - Duration::minutes(15);
        inside.input_tokens = 2400;
        inside.output_tokens = 600;

        let mut outside = create_dummy_record(9.0, 0);
        outside.timestamp = now - Duration::minutes(90);
        outside.input_tokens = 10_000;

        let rate = compute_burn_rate_at(&[inside, outside], now, Duration::minutes(60));
        assert!(rate.is_active());
        assert_eq!(rate.window_tokens, 3000);
        assert!((rate.window_cost - 2.5).abs() < 1e-9);
        assert!((rate.cost_per_hour - 2.5).abs() < 1e-9);
        assert!((rate.tokens_per_min - 50.0).abs() < 1e-9);
        assert_eq!(format_burn_rate(&rate), "$2.50/hr · 50/m");

        let inactive = compute_burn_rate_at(&[], now, Duration::minutes(60));
        assert!(!inactive.is_active());
        assert_eq!(format_burn_rate(&inactive), "$0.00/hr · 0/m");
    }
}
