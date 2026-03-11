use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};

use crate::display;
use crate::types::{GroupBy, ModelUsage, PeriodSummary, Record, SessionSummary};

/// Group entries by date, then by model within each date
pub fn aggregate_daily(entries: &[Record]) -> Vec<PeriodSummary> {
    let grouped = group_by_date(entries, |e| {
        let date = e.timestamp.date_naive();
        (date, date.format("%Y-%m-%d").to_string())
    });
    build_summaries(grouped)
}

/// Group entries by ISO week
pub fn aggregate_weekly(entries: &[Record]) -> Vec<PeriodSummary> {
    let grouped = group_by_date(entries, |e| {
        let date = e.timestamp.date_naive();
        let iso = date.iso_week();
        let year = iso.year();
        let week = iso.week();
        // Use Monday of the ISO week as the representative date
        let monday = NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon).unwrap_or(date);
        let sunday = monday + chrono::Duration::days(6);
        let label = format!(
            "{}-W{:02} ({} - {})",
            year,
            week,
            monday.format("%b %d"),
            sunday.format("%b %d")
        );
        (monday, label)
    });
    build_summaries(grouped)
}

/// Group entries by month
pub fn aggregate_monthly(entries: &[Record]) -> Vec<PeriodSummary> {
    let grouped = group_by_date(entries, |e| {
        let date = e.timestamp.date_naive();
        let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date);
        let label = date.format("%B %Y").to_string();
        (first, label)
    });
    build_summaries(grouped)
}

/// Apply date range filter
pub fn filter_by_date(
    entries: Vec<Record>,
    since: Option<NaiveDate>,
    until: Option<NaiveDate>,
) -> Vec<Record> {
    entries
        .into_iter()
        .filter(|e| {
            let date = e.timestamp.date_naive();
            since.is_none_or(|s| date >= s) && until.is_none_or(|u| date <= u)
        })
        .collect()
}

/// Group entries by session_id, compute totals per session
pub fn aggregate_by_session(entries: &[Record]) -> Vec<SessionSummary> {
    let mut grouped: HashMap<&str, Vec<&Record>> = HashMap::new();

    for entry in entries {
        if let Some(ref sid) = entry.session_id {
            grouped.entry(sid.as_str()).or_default().push(entry);
        }
    }

    let mut sessions: Vec<SessionSummary> = grouped
        .into_iter()
        .map(|(sid, records)| {
            let mut input = 0u64;
            let mut output = 0u64;
            let mut cache_read = 0u64;
            let mut cache_creation = 0u64;
            let mut thinking = 0u64;
            let mut cost = 0.0f64;
            let mut model_tokens: HashMap<&str, u64> = HashMap::new();
            let mut earliest = records[0].timestamp;
            let mut client: &str = &records[0].provider;

            for r in &records {
                input += r.input_tokens;
                output += r.output_tokens;
                cache_read += r.cache_read_tokens;
                cache_creation += r.cache_creation_tokens;
                thinking += r.thinking_tokens;
                cost += r.cost_usd.unwrap_or(0.0);

                let model = r.model.as_deref().unwrap_or("unknown");
                *model_tokens.entry(model).or_default() += r.total_tokens();

                if r.timestamp < earliest {
                    earliest = r.timestamp;
                    client = &r.provider;
                }
            }

            let total = input + output + cache_read + cache_creation + thinking;

            let dominant_model = model_tokens
                .into_iter()
                .max_by_key(|(_, tokens)| *tokens)
                .map_or("unknown", |(m, _)| m);

            SessionSummary {
                session_id: sid.to_string(),
                date: earliest.date_naive(),
                client: display::display_client(client).into_owned(),
                dominant_model: display::display_model(dominant_model),
                input_tokens: input,
                output_tokens: output,
                cache_read_tokens: cache_read,
                cache_creation_tokens: cache_creation,
                thinking_tokens: thinking,
                total_tokens: total,
                cost,
            }
        })
        .collect();

    // Sort by cost descending
    sessions.sort_unstable_by(|a, b| b.cost.total_cmp(&a.cost));
    sessions
}

fn group_by_date<F>(entries: &[Record], key_fn: F) -> BTreeMap<NaiveDate, (String, Vec<&Record>)>
where
    F: Fn(&Record) -> (NaiveDate, String),
{
    let mut grouped: BTreeMap<NaiveDate, (String, Vec<&Record>)> = BTreeMap::new();
    for entry in entries {
        let (date, label) = key_fn(entry);
        grouped
            .entry(date)
            .or_insert_with(|| (label, Vec::new()))
            .1
            .push(entry);
    }
    grouped
}

/// Aggregate `PeriodSummary` model usages into a flat `Vec<ModelUsage>`
/// grouped by the selected `GroupBy` mode. Used by both `recompute_detail`
/// and `compute_all_time_base`.
pub fn aggregate_summaries_to_models(
    summaries: &[PeriodSummary],
    group_by: GroupBy,
) -> Vec<ModelUsage> {
    let mut model_map: HashMap<(String, String), ModelUsage> = HashMap::new();

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
            entry.accumulate(mu);
        }
    }

    model_map.into_values().collect()
}

/// Merge two sets of `ModelUsage` by summing values for matching keys.
pub fn merge_model_usages(base: &[ModelUsage], window: &[ModelUsage]) -> Vec<ModelUsage> {
    let mut map: HashMap<(String, String), ModelUsage> = HashMap::new();

    for mu in base.iter().chain(window.iter()) {
        let key = (mu.model.clone(), mu.provider.clone());
        let entry = map.entry(key).or_insert_with(|| ModelUsage {
            model: mu.model.clone(),
            raw_model: mu.raw_model.clone(),
            provider: mu.provider.clone(),
            ..Default::default()
        });
        entry.accumulate(mu);
    }

    map.into_values().collect()
}

fn build_summaries(grouped: BTreeMap<NaiveDate, (String, Vec<&Record>)>) -> Vec<PeriodSummary> {
    let mut summaries = Vec::new();

    for (date, (label, entries)) in grouped {
        // Key by (provider, normalized_model) so the same model used via
        // different routing prefixes (e.g., "vertexai.claude-opus-4-6"
        // from OpenCode vs "claude-opus-4-6" from Claude Code) still
        // aggregates as separate rows per client, but doesn't create
        // duplicate model entries within the same client.
        let mut model_map: HashMap<(String, String), ModelUsage> = HashMap::new();

        for entry in &entries {
            let raw_model = entry.model.as_deref().unwrap_or("unknown");
            let norm = display::normalize_model(raw_model);
            let key = (entry.provider.to_string(), norm.clone());

            let mu = model_map.entry(key).or_insert_with(|| ModelUsage {
                model: norm,
                raw_model: raw_model.to_string(),
                provider: entry.provider.to_string(),
                ..Default::default()
            });

            mu.accumulate(&ModelUsage {
                input_tokens: entry.input_tokens,
                output_tokens: entry.output_tokens,
                cache_read_tokens: entry.cache_read_tokens,
                cache_creation_tokens: entry.cache_creation_tokens,
                thinking_tokens: entry.thinking_tokens,
                cost_usd: entry.cost_usd.unwrap_or(0.0),
                request_count: 1,
                ..Default::default()
            });
        }

        let models: Vec<ModelUsage> = model_map.into_values().collect();

        let total_input: u64 = models.iter().map(|m| m.input_tokens).sum();
        let total_output: u64 = models.iter().map(|m| m.output_tokens).sum();
        let total_thinking: u64 = models.iter().map(|m| m.thinking_tokens).sum();
        let total_cost: f64 = models.iter().map(|m| m.cost_usd).sum();
        let total_requests: u64 = models.iter().map(|m| m.request_count).sum();

        summaries.push(PeriodSummary {
            date,
            label,
            models,
            total_input,
            total_output,
            total_thinking,
            total_cost,
            total_requests,
        });
    }

    summaries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GroupBy, ModelUsage, Record};
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::borrow::Cow;

    fn make_record(
        timestamp_sec: i64,
        model: &str,
        provider: &str,
        input: u64,
        output: u64,
    ) -> Record {
        Record {
            timestamp: Utc.timestamp_opt(timestamp_sec, 0).unwrap(),
            provider: Cow::Owned(provider.to_string()),
            model: Some(model.to_string()),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            thinking_tokens: 0,
            cost_usd: Some(0.1),
            message_id: None,
            request_id: None,
            session_id: None,
        }
    }

    #[test]
    fn test_aggregate_daily() {
        // Records on two different days
        // Day 1: 2023-01-01T00:00:00Z (1672531200)
        // Day 2: 2023-01-02T00:00:00Z (1672617600)
        let r1 = make_record(1672531200, "model-a", "prov-a", 10, 20); // Day 1
        let r2 = make_record(1672534800, "model-a", "prov-a", 15, 25); // Day 1
        let r3 = make_record(1672617600, "model-b", "prov-b", 100, 200); // Day 2

        let summaries = aggregate_daily(&[r1, r2, r3]);
        assert_eq!(summaries.len(), 2);

        // Check Day 1
        let day1 = summaries
            .iter()
            .find(|s| s.date == NaiveDate::from_ymd_opt(2023, 1, 1).unwrap())
            .unwrap();
        assert_eq!(day1.total_input, 25);
        assert_eq!(day1.total_output, 45);
        assert_eq!(day1.total_requests, 2);
        assert_eq!(day1.models.len(), 1);
        assert_eq!(day1.models[0].model, "model-a");

        // Check Day 2
        let day2 = summaries
            .iter()
            .find(|s| s.date == NaiveDate::from_ymd_opt(2023, 1, 2).unwrap())
            .unwrap();
        assert_eq!(day2.total_input, 100);
        assert_eq!(day2.total_requests, 1);
        assert_eq!(day2.models.len(), 1);
    }

    #[test]
    fn test_merge_model_usages() {
        let mu1 = ModelUsage {
            model: "m1".to_string(),
            raw_model: "raw-m1".to_string(),
            provider: "p1".to_string(),
            input_tokens: 10,
            ..Default::default()
        };
        let mu2 = ModelUsage {
            model: "m2".to_string(),
            raw_model: "raw-m2".to_string(),
            provider: "p1".to_string(),
            input_tokens: 20,
            ..Default::default()
        };
        let mu3 = ModelUsage {
            // matches mu1 key
            model: "m1".to_string(),
            raw_model: "raw-m1".to_string(),
            provider: "p1".to_string(),
            input_tokens: 30,
            ..Default::default()
        };

        let base = vec![mu1.clone(), mu2.clone()];
        let window = vec![mu3.clone()];

        let merged = merge_model_usages(&base, &window);
        assert_eq!(merged.len(), 2);

        let m1_merged = merged.iter().find(|m| m.model == "m1").unwrap();
        assert_eq!(m1_merged.input_tokens, 40); // 10 + 30

        let m2_merged = merged.iter().find(|m| m.model == "m2").unwrap();
        assert_eq!(m2_merged.input_tokens, 20);
    }

    #[test]
    fn test_aggregate_summaries_to_models() {
        let mu1 = ModelUsage {
            model: "m1".to_string(),
            raw_model: "raw-m1".to_string(),
            provider: "p1".to_string(),
            input_tokens: 10,
            ..Default::default()
        };
        let mu2 = ModelUsage {
            model: "m1".to_string(),
            raw_model: "raw-m1".to_string(),
            provider: "p2".to_string(), // different provider, same model
            input_tokens: 20,
            ..Default::default()
        };
        let mu3 = ModelUsage {
            model: "m2".to_string(),
            raw_model: "raw-m2".to_string(),
            provider: "p1".to_string(),
            input_tokens: 30,
            ..Default::default()
        };

        let summary = PeriodSummary {
            date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
            label: "test".to_string(),
            models: vec![mu1, mu2, mu3],
            total_input: 60,
            total_output: 0,
            total_thinking: 0,
            total_cost: 0.0,
            total_requests: 3,
        };
        let summaries = vec![summary];

        // GroupBy::Model
        let by_model = aggregate_summaries_to_models(&summaries, GroupBy::Model);
        assert_eq!(by_model.len(), 2); // m1 and m2
        let m1_agg = by_model.iter().find(|m| m.model == "m1").unwrap();
        assert_eq!(m1_agg.input_tokens, 30); // p1 (10) + p2 (20)

        // GroupBy::ModelClient
        let by_model_client = aggregate_summaries_to_models(&summaries, GroupBy::ModelClient);
        assert_eq!(by_model_client.len(), 3); // m1+p1, m1+p2, m2+p1

        // GroupBy::Client
        let by_client = aggregate_summaries_to_models(&summaries, GroupBy::Client);
        assert_eq!(by_client.len(), 2); // p1 and p2
        let p1_agg = by_client.iter().find(|m| m.provider == "p1").unwrap();
        assert_eq!(p1_agg.input_tokens, 40); // m1 (10) + m2 (30)
    }
}
