use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::theme;

/// Render the bottom status bar with keybinding hints or filter input.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Fill the background with surface colour
    let bg = ratatui::widgets::Block::default().style(theme::status_bar());
    frame.render_widget(bg, area);

    if app.filter_active {
        // Show filter input
        let line = Line::from(vec![
            Span::styled("/", theme::status_key()),
            Span::styled(&app.filter_text, theme::text_bold()),
            Span::styled("█", theme::status_key()), // cursor
        ]);
        frame.render_widget(line, area);
        return;
    }

    // Show warning if one is active (overrides keybinding hints)
    if let Some(warning) = app.active_warning() {
        let warning_style = Style::default()
            .fg(theme::YELLOW)
            .bg(theme::SURFACE)
            .add_modifier(Modifier::BOLD);
        let line = Line::from(vec![
            Span::styled("⚠ ", warning_style),
            Span::styled(warning.to_string(), warning_style),
        ]);
        frame.render_widget(line, area);
        return;
    }

    let mut spans: Vec<Span> = Vec::new();

    // Show active filter if any
    if !app.applied_filter.is_empty() {
        spans.push(Span::styled("filter:", theme::status_key()));
        spans.push(Span::styled(
            format!("{} ", &app.applied_filter),
            Style::default().fg(theme::YELLOW).bg(theme::SURFACE),
        ));
        spans.push(Span::styled(" │ ", theme::status_bar()));
    }

    let sort_label = format!("sort:{}", app.sort_order.label());
    let group_label = format!("group:{}", app.group_by.label());
    let bindings: Vec<(&str, &str)> = vec![
        ("t/w/m/a", "scope"),
        ("T/W/M/A", "cards"),
        ("g", &group_label),
        ("h", "history"),
        ("s", &sort_label),
        ("/", "filter"),
        ("j/k", "scroll"),
        ("S", "settings"),
        ("?", "help"),
        ("q", "quit"),
    ];

    for (i, (key, desc)) in bindings.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", theme::status_bar()));
        }
        spans.push(Span::styled(*key, theme::status_key()));
        spans.push(Span::styled(format!(" {desc}"), theme::status_bar()));
    }

    let line = Line::from(spans);
    frame.render_widget(line, area);
}
