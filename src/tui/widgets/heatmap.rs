use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, NaiveDate, Utc};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::display;
use crate::tui::theme;
use crate::types::Record;

const LABEL_WIDTH: u16 = 4;
const WEEKS_IN_YEAR: usize = 53;

/// Aggregated usage for one day in the contribution grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapDay {
    pub date: NaiveDate,
    pub total_tokens: u64,
    pub dominant_provider: String,
}

#[derive(Default)]
struct DayAccumulator {
    total_tokens: u64,
    provider_tokens: BTreeMap<String, u64>,
}

/// Aggregate records into chronological per-day contributions.
#[must_use]
pub fn build_heatmap_data(records: &[Record]) -> Vec<HeatmapDay> {
    let mut days: BTreeMap<NaiveDate, DayAccumulator> = BTreeMap::new();

    for record in records {
        let tokens = record.total_tokens();
        if tokens == 0 {
            continue;
        }

        let provider = provider_name(record);
        let day = days.entry(record.timestamp.date_naive()).or_default();
        day.total_tokens = day.total_tokens.saturating_add(tokens);
        let entry = day.provider_tokens.entry(provider).or_default();
        *entry = entry.saturating_add(tokens);
    }

    days.into_iter()
        .map(|(date, day)| {
            let dominant_provider = day
                .provider_tokens
                .into_iter()
                .max_by(|(provider_a, tokens_a), (provider_b, tokens_b)| {
                    tokens_a
                        .cmp(tokens_b)
                        .then_with(|| provider_b.cmp(provider_a))
                })
                .map_or_else(|| "Other".to_string(), |(provider, _)| provider);
            HeatmapDay {
                date,
                total_tokens: day.total_tokens,
                dominant_provider,
            }
        })
        .collect()
}

fn provider_name(record: &Record) -> String {
    let provider = display::infer_api_provider(record.model.as_deref().unwrap_or(""));
    if provider.is_empty() {
        "Other".to_string()
    } else {
        provider.to_string()
    }
}

/// Render a responsive contribution heatmap for the twelve months ending today.
pub fn render(frame: &mut Frame, area: Rect, data: &[HeatmapDay]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(
            " Contributions · last 12 months ",
            theme::header(),
        ))
        .style(theme::text());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 18 || inner.height < 8 {
        render_message(frame, inner, "Terminal too small for contribution heatmap");
        return;
    }
    if data.is_empty() {
        render_message(frame, inner, "No usage data for the last 12 months");
        return;
    }

    let today = Utc::now().date_naive();
    let available_width = inner.width.saturating_sub(LABEL_WIDTH);
    let cell_width = u16::from(available_width >= (WEEKS_IN_YEAR as u16 * 2));
    let cell_width = cell_width + 1;
    let visible_weeks = ((available_width / cell_width) as usize).min(WEEKS_IN_YEAR);
    if visible_weeks == 0 {
        render_message(frame, inner, "Terminal too small for contribution heatmap");
        return;
    }

    let current_monday = monday_of_week(today);
    let display_start = current_monday - Duration::weeks((visible_weeks - 1) as i64);
    let day_map: HashMap<NaiveDate, &HeatmapDay> = data.iter().map(|day| (day.date, day)).collect();
    let max_tokens = data
        .iter()
        .filter(|day| day.date >= display_start && day.date <= today)
        .map(|day| day.total_tokens)
        .max()
        .unwrap_or(0);

    let month_labels = build_month_labels(display_start, visible_weeks, cell_width as usize);
    frame.render_widget(
        Line::from(vec![
            Span::raw(" ".repeat(LABEL_WIDTH as usize)),
            Span::styled(month_labels, theme::text_dim()),
        ]),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let labels = ["Mon", "", "Wed", "", "Fri", "", ""];
    for (day_index, label) in labels.iter().enumerate() {
        let mut spans = Vec::with_capacity(visible_weeks + 1);
        spans.push(Span::styled(
            format!("{label:<width$}", width = LABEL_WIDTH as usize),
            theme::text_dim(),
        ));

        for week in 0..visible_weeks {
            let cell_date =
                display_start + Duration::weeks(week as i64) + Duration::days(day_index as i64);
            if cell_date > today {
                spans.push(Span::raw(" ".repeat(cell_width as usize)));
            } else if let Some(day) = day_map.get(&cell_date) {
                let level = intensity_level(day.total_tokens, max_tokens);
                let color = intensity_color(theme::provider_color(&day.dominant_provider), level);
                spans.push(Span::styled(
                    "█".repeat(cell_width as usize),
                    Style::default().fg(color),
                ));
            } else {
                let empty = if cell_width == 1 { "·" } else { "· " };
                spans.push(Span::styled(empty, Style::default().fg(theme::BORDER)));
            }
        }

        frame.render_widget(
            Line::from(spans),
            Rect::new(inner.x, inner.y + 1 + day_index as u16, inner.width, 1),
        );
    }
}

fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    frame.render_widget(Paragraph::new(message).style(theme::text_dim()), area);
}

fn monday_of_week(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

fn build_month_labels(start: NaiveDate, weeks: usize, cell_width: usize) -> String {
    let mut line = vec![b' '; weeks * cell_width];
    let mut previous_month = None;

    for week in 0..weeks {
        let date = start + Duration::weeks(week as i64);
        if previous_month == Some(date.month()) {
            continue;
        }
        previous_month = Some(date.month());
        let label = date.format("%b").to_string();
        let offset = week * cell_width;
        for (index, byte) in label.bytes().enumerate() {
            if let Some(slot) = line.get_mut(offset + index) {
                *slot = byte;
            }
        }
    }

    String::from_utf8(line).unwrap_or_default()
}

fn intensity_level(tokens: u64, max_tokens: u64) -> u8 {
    if tokens == 0 || max_tokens == 0 {
        return 0;
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let level = ((tokens as f64 / max_tokens as f64).sqrt() * 4.0).ceil() as u8;
    level.clamp(1, 4)
}

fn intensity_color(color: Color, level: u8) -> Color {
    let scale = match level {
        0 => 0.0,
        1 => 0.3,
        2 => 0.5,
        3 => 0.75,
        _ => 1.0,
    };
    match color {
        Color::Rgb(red, green, blue) =>
        {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Color::Rgb(
                (f64::from(red) * scale) as u8,
                (f64::from(green) * scale) as u8,
                (f64::from(blue) * scale) as u8,
            )
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use chrono::{TimeZone, Utc};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;

    fn record(day: u32, model: &str, input: u64, output: u64) -> Record {
        Record {
            timestamp: Utc.with_ymd_and_hms(2026, 7, day, 12, 0, 0).unwrap(),
            provider: Cow::Borrowed("test"),
            model: Some(model.to_string()),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            thinking_tokens: 0,
            cost_usd: None,
            message_id: None,
            request_id: None,
            session_id: None,
        }
    }

    #[test]
    fn aggregates_days_and_selects_dominant_provider() {
        let records = vec![
            record(2, "claude-opus-5", 100, 50),
            record(2, "gpt-5", 400, 50),
            record(1, "gemini-2.5-flash", 40, 10),
        ];

        let days = build_heatmap_data(&records);

        assert_eq!(days.len(), 2);
        assert_eq!(days[0].date, NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
        assert_eq!(days[0].total_tokens, 50);
        assert_eq!(days[0].dominant_provider, "Google");
        assert_eq!(days[1].total_tokens, 600);
        assert_eq!(days[1].dominant_provider, "OpenAI");
    }

    #[test]
    fn intensity_is_bounded_and_handles_empty_data() {
        assert_eq!(intensity_level(0, 0), 0);
        assert_eq!(intensity_level(1, 100), 1);
        assert_eq!(intensity_level(25, 100), 2);
        assert_eq!(intensity_level(100, 100), 4);
    }

    #[test]
    fn empty_and_small_render_areas_are_safe() {
        for (width, height) in [(10, 3), (40, 12)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render(frame, area, &[]);
                })
                .unwrap();
        }
    }

    #[test]
    fn populated_heatmap_adapts_to_available_width() {
        let data = [HeatmapDay {
            date: Utc::now().date_naive(),
            total_tokens: 1_000,
            dominant_provider: "Anthropic".to_string(),
        }];

        for width in [20, 120] {
            let backend = TestBackend::new(width, 12);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render(frame, area, &data);
                })
                .unwrap();
        }
    }
}
