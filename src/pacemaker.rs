use chrono::NaiveDate;

use crate::config::BudgetConfig;
use crate::timestamp;
use crate::types::Record;

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

/// Evaluate spending against budget limits.
/// Returns a [`BudgetStatus`] with spending and limits for each configured period.
pub fn evaluate(entries: &[Record], budget: &BudgetConfig) -> BudgetStatus {
    let daily = budget.daily.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_today());
        BudgetPeriod { spent, limit }
    });

    let weekly = budget.weekly.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_week());
        BudgetPeriod { spent, limit }
    });

    let monthly = budget.monthly.map(|limit| {
        let spent = sum_cost_since(entries, timestamp::start_of_month());
        BudgetPeriod { spent, limit }
    });

    BudgetStatus {
        daily,
        weekly,
        monthly,
    }
}

fn sum_cost_since(entries: &[Record], since: NaiveDate) -> f64 {
    entries
        .iter()
        .filter(|e| e.timestamp.date_naive() >= since)
        .filter_map(|e| e.cost_usd)
        .sum()
}
