use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};

use crate::display;
use crate::types::{DailySummary, GroupBy, ModelUsage, Record, SessionSummary};

/// Group entries by date, then by model within each date
pub fn aggregate_daily(entries: &[Record]) -> Vec<DailySummary> {
    let grouped = group_by_date(entries, |e| {
        let date = e.timestamp.date_naive();
        (date, date.format("%Y-%m-%d").to_string())
    });
    build_summaries(grouped)
}

/// Group entries by ISO week
pub fn aggregate_weekly(entries: &[Record]) -> Vec<DailySummary> {
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
pub fn aggregate_monthly(entries: &[Record]) -> Vec<DailySummary> {
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

/// Aggregate `DailySummary` model usages into a flat `Vec<ModelUsage>`
/// grouped by the selected `GroupBy` mode. Used by both `recompute_detail`
/// and `compute_all_time_base`.
pub fn aggregate_summaries_to_models(
    summaries: &[DailySummary],
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

fn build_summaries(grouped: BTreeMap<NaiveDate, (String, Vec<&Record>)>) -> Vec<DailySummary> {
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

        summaries.push(DailySummary {
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
