use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::theme;

/// Render the top header bar.
///
/// ```text
/// ◆ tokemon                                        10 Mar 2026
/// ```
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(16)]).areas(area);

    // Left: branding + live burn rate when active
    let mut brand_spans = vec![
        Span::styled("◆ ", theme::header()),
        Span::styled("tokemon", theme::text_bold()),
    ];
    if app.burn_rate.is_active() {
        brand_spans.push(Span::styled("  ·  ", theme::text_dim()));
        brand_spans.push(Span::styled(
            crate::pacemaker::format_burn_rate(&app.burn_rate),
            theme::text_dim(),
        ));
    }
    let brand = Line::from(brand_spans);
    frame.render_widget(brand, left);

    // Right: current date
    let now = Utc::now();
    let date_str = now.format("%d %b %Y").to_string();
    let date_line = Line::from(Span::styled(date_str, theme::text_dim())).right_aligned();
    frame.render_widget(date_line, right);
}
