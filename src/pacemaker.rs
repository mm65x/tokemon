use chrono::{Datelike, NaiveDate, Utc};

use crate::config::BudgetConfig;
use crate::types::Record;

/// Evaluate spending against budget limits.
/// Returns (spent, limit) pairs for each configured budget period.
#[allow(clippy::type_complexity)]
pub fn evaluate(
    entries: &[Record],
    budget: &BudgetConfig,
) -> (Option<(f64, f64)>, Option<(f64, f64)>, Option<(f64, f64)>) {
    let today = Utc::now().date_naive();

    let daily = budget.daily.map(|limit| {
        let spent = sum_cost_since(entries, today);
        (spent, limit)
    });

    let weekly = budget.weekly.map(|limit| {
        let week_start =
            today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()));
        let spent = sum_cost_since(entries, week_start);
        (spent, limit)
    });

    let monthly = budget.monthly.map(|limit| {
        let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        let spent = sum_cost_since(entries, month_start);
        (spent, limit)
    });

    (daily, weekly, monthly)
}

fn sum_cost_since(entries: &[Record], since: NaiveDate) -> f64 {
    entries
        .iter()
        .filter(|e| e.timestamp.date_naive() >= since)
        .filter_map(|e| e.cost_usd)
        .sum()
}
