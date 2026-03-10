use chrono::NaiveDate;

use crate::config::BudgetConfig;
use crate::timestamp;
use crate::types::Record;

/// Evaluate spending against budget limits.
/// Returns (spent, limit) pairs for each configured budget period.
#[allow(clippy::type_complexity)]
pub fn evaluate(
    entries: &[Record],
    budget: &BudgetConfig,
) -> (Option<(f64, f64)>, Option<(f64, f64)>, Option<(f64, f64)>) {
    let daily = budget.daily.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_today());
        (spent, limit)
    });

    let weekly = budget.weekly.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_week());
        (spent, limit)
    });

    let monthly = budget.monthly.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_month());
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
