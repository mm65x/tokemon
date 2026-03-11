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
        assert_eq!(sum_cost_since(&entries, since_today), 3.0);
    }
}
