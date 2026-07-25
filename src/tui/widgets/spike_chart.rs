use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::display;
use crate::render::format_tokens_short;
use crate::tui::theme;
use crate::types::Record;

/// One token-usage bucket in the spike chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpikeBucket {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub dominant_provider: String,
}

impl SpikeBucket {
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// A bounded time series ready for responsive rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpikeSeries {
    pub start: DateTime<Utc>,
    pub bucket_seconds: u32,
    pub buckets: Vec<SpikeBucket>,
}

/// Aggregate records into fixed-width time buckets.
#[must_use]
pub fn build_spike_data(
    records: &[Record],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    bucket_seconds: u32,
) -> SpikeSeries {
    let bucket_seconds = bucket_seconds.max(1);
    let span_seconds = (end - start).num_seconds().max(0);
    let bucket_count = if span_seconds == 0 {
        0
    } else {
        ((span_seconds + i64::from(bucket_seconds) - 1) / i64::from(bucket_seconds)) as usize
    };
    let mut input = vec![0_u64; bucket_count];
    let mut output = vec![0_u64; bucket_count];
    let mut providers: Vec<BTreeMap<String, u64>> = vec![BTreeMap::new(); bucket_count];

    for record in records {
        if record.timestamp < start || record.timestamp >= end {
            continue;
        }
        let offset = (record.timestamp - start).num_seconds();
        let index = (offset / i64::from(bucket_seconds)) as usize;
        if index >= bucket_count {
            continue;
        }

        let input_tokens = record
            .input_tokens
            .saturating_add(record.cache_read_tokens)
            .saturating_add(record.cache_creation_tokens);
        let output_tokens = record.output_tokens.saturating_add(record.thinking_tokens);
        input[index] = input[index].saturating_add(input_tokens);
        output[index] = output[index].saturating_add(output_tokens);

        let provider = provider_name(record);
        let total = input_tokens.saturating_add(output_tokens);
        let entry = providers[index].entry(provider).or_default();
        *entry = entry.saturating_add(total);
    }

    let buckets = input
        .into_iter()
        .zip(output)
        .zip(providers)
        .map(|((input_tokens, output_tokens), provider_tokens)| {
            let dominant_provider = provider_tokens
                .into_iter()
                .max_by(|(provider_a, tokens_a), (provider_b, tokens_b)| {
                    tokens_a
                        .cmp(tokens_b)
                        .then_with(|| provider_b.cmp(provider_a))
                })
                .map_or_else(|| "Other".to_string(), |(provider, _)| provider);
            SpikeBucket {
                input_tokens,
                output_tokens,
                dominant_provider,
            }
        })
        .collect();

    SpikeSeries {
        start,
        bucket_seconds,
        buckets,
    }
}

fn provider_name(record: &Record) -> String {
    let provider = display::infer_api_provider(record.model.as_deref().unwrap_or(""));
    if provider.is_empty() {
        "Other".to_string()
    } else {
        provider.to_string()
    }
}

/// Render recent token volume as provider-coloured spikes.
pub fn render(frame: &mut Frame, area: Rect, series: Option<&SpikeSeries>) {
    let bucket_label = series.map_or(5, |data| data.bucket_seconds / 60);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(
            format!(" Token spikes · {bucket_label}m buckets "),
            theme::header(),
        ))
        .style(theme::text());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 12 || inner.height < 4 {
        render_message(frame, inner, "Terminal too small for spike chart");
        return;
    }
    let Some(series) = series else {
        render_message(frame, inner, "No token activity for today");
        return;
    };

    let visible_count = (inner.width as usize).min(series.buckets.len());
    let offset = series.buckets.len().saturating_sub(visible_count);
    let visible = &series.buckets[offset..];
    let max_tokens = visible
        .iter()
        .map(SpikeBucket::total_tokens)
        .max()
        .unwrap_or(0);
    if max_tokens == 0 {
        render_message(frame, inner, "No token activity for today");
        return;
    }

    let chart_height = inner.height.saturating_sub(1) as usize;
    for row in 0..chart_height {
        let mut spans = Vec::with_capacity(visible_count);
        for bucket in visible {
            let tokens = bucket.total_tokens();
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let height =
                ((tokens as f64 / max_tokens as f64) * chart_height as f64).ceil() as usize;
            let filled = height > 0 && row >= chart_height.saturating_sub(height);
            if filled {
                spans.push(Span::styled(
                    "█",
                    Style::default().fg(theme::provider_color(&bucket.dominant_provider)),
                ));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        frame.render_widget(
            Line::from(spans),
            Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
        );
    }

    let visible_start =
        series.start + Duration::seconds((offset as i64) * i64::from(series.bucket_seconds));
    let totals = visible
        .iter()
        .fold((0_u64, 0_u64), |(input, output), bucket| {
            (
                input.saturating_add(bucket.input_tokens),
                output.saturating_add(bucket.output_tokens),
            )
        });
    let footer = format!(
        "{}  in {}  out {}  → now",
        visible_start.format("%H:%M"),
        format_tokens_short(totals.0),
        format_tokens_short(totals.1)
    );
    frame.render_widget(
        Paragraph::new(footer).style(theme::text_dim()),
        Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        ),
    );
}

fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    frame.render_widget(Paragraph::new(message).style(theme::text_dim()), area);
}

/// Return the start of the UTC day containing `timestamp`.
#[must_use]
pub fn start_of_day(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .with_hour(0)
        .and_then(|value| value.with_minute(0))
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(timestamp)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use chrono::{TimeZone, Utc};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;

    fn record(
        minute: u32,
        model: &str,
        input: u64,
        output: u64,
        cache: u64,
        thinking: u64,
    ) -> Record {
        Record {
            timestamp: Utc.with_ymd_and_hms(2026, 7, 25, 10, minute, 0).unwrap(),
            provider: Cow::Borrowed("test"),
            model: Some(model.to_string()),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache,
            cache_creation_tokens: 0,
            thinking_tokens: thinking,
            cost_usd: None,
            message_id: None,
            request_id: None,
            session_id: None,
        }
    }

    #[test]
    fn buckets_records_and_counts_all_token_classes() {
        let start = Utc.with_ymd_and_hms(2026, 7, 25, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 25, 11, 0, 0).unwrap();
        let records = vec![
            record(0, "claude-opus-5", 100, 20, 30, 5),
            record(14, "claude-opus-5", 10, 2, 3, 1),
            record(15, "gpt-5", 200, 40, 0, 10),
            record(59, "gemini-2.5-flash", 50, 5, 0, 0),
        ];

        let series = build_spike_data(&records, start, end, 15 * 60);

        assert_eq!(series.buckets.len(), 4);
        assert_eq!(series.buckets[0].input_tokens, 143);
        assert_eq!(series.buckets[0].output_tokens, 28);
        assert_eq!(series.buckets[0].dominant_provider, "Anthropic");
        assert_eq!(series.buckets[1].total_tokens(), 250);
        assert_eq!(series.buckets[3].dominant_provider, "Google");
    }

    #[test]
    fn excludes_records_outside_the_requested_range() {
        let start = Utc.with_ymd_and_hms(2026, 7, 25, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 25, 10, 15, 0).unwrap();
        let records = vec![
            record(0, "gpt-5", 10, 0, 0, 0),
            record(15, "gpt-5", 100, 0, 0, 0),
        ];

        let series = build_spike_data(&records, start, end, 15 * 60);

        assert_eq!(series.buckets.len(), 1);
        assert_eq!(series.buckets[0].total_tokens(), 10);
    }

    #[test]
    fn empty_and_small_render_areas_are_safe() {
        for (width, height) in [(8, 3), (40, 12)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render(frame, area, None);
                })
                .unwrap();
        }
    }

    #[test]
    fn populated_spikes_adapt_to_available_width() {
        let start = Utc.with_ymd_and_hms(2026, 7, 25, 10, 0, 0).unwrap();
        let series = SpikeSeries {
            start,
            bucket_seconds: 300,
            buckets: (0..100)
                .map(|index| SpikeBucket {
                    input_tokens: index * 10,
                    output_tokens: index * 5,
                    dominant_provider: "OpenAI".to_string(),
                })
                .collect(),
        };

        for width in [20, 120] {
            let backend = TestBackend::new(width, 12);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render(frame, area, Some(&series));
                })
                .unwrap();
        }
    }
}
