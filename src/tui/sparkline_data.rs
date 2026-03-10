use chrono::{Datelike, NaiveDate, Timelike, Utc};

use crate::types::Record;

// ── Helpers ───────────────────────────────────────────────────────────────

pub(crate) fn sum_cost(records: &[&Record]) -> f64 {
    records.iter().map(|r| r.cost_usd.unwrap_or(0.0)).sum()
}

pub(crate) fn sum_tokens(records: &[&Record]) -> u64 {
    records.iter().map(|r| r.total_tokens()).sum()
}

/// Extract the sparkline metric value from a record.
/// For "cost" mode, scales USD to millicents (x100_000) so small values are visible.
pub(crate) fn sparkline_value(record: &Record, use_cost: bool) -> u64 {
    if use_cost {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (record.cost_usd.unwrap_or(0.0) * 100_000.0) as u64;
        v
    } else {
        record.total_tokens()
    }
}

/// Build a sparkline of N-minute buckets for today.
/// `bucket_mins` controls granularity (e.g. 1, 5, 10, 30, 60).
pub(crate) fn build_minute_sparkline(
    records: &[Record],
    bucket_mins: u32,
    use_cost: bool,
) -> Vec<u64> {
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
pub(crate) fn build_hour_sparkline(
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
pub(crate) fn build_day_sparkline(
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
pub(crate) fn build_weekly_sparkline_data(
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
pub(crate) fn iso_week_diff(start: (i32, u32), end: (i32, u32)) -> usize {
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
pub(crate) fn merge_weekly_sparklines(
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
pub(crate) fn compute_trend(data: &[u64]) -> i8 {
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
