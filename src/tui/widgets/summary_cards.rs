use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Sparkline};
use ratatui::Frame;

use crate::tui::app::{App, HoverTarget, Scope};
use crate::tui::theme;

/// Render the four summary cards: Today, This Week, This Month, All Time.
///
/// Each card shows:
/// - Label (highlighted if it matches the active scope)
/// - Cost (large, bold)
/// - Token count (secondary)
/// - Sparkline (trend)
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let show_sparklines = app.config.show_sparklines;
    for (i, (scope, card_area)) in card_areas(area).into_iter().enumerate() {
        render_card(
            frame,
            card_area,
            &app.cards[i],
            scope == app.scope,
            matches!(app.hovered.as_ref(), Some(HoverTarget::Card(hovered)) if *hovered == scope),
            show_sparklines,
        );
    }
}

/// Return the scope card rectangles in their rendered order.
#[must_use]
pub(crate) fn card_areas(area: Rect) -> [(Scope, Rect); 4] {
    let [today, week, month, all_time] = Layout::horizontal([
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
    ])
    .areas(area);

    [
        (Scope::Today, today),
        (Scope::Week, week),
        (Scope::Month, month),
        (Scope::AllTime, all_time),
    ]
}

/// Return the scope card at a terminal coordinate, if any.
#[must_use]
pub(crate) fn scope_at(area: Rect, column: u16, row: u16) -> Option<Scope> {
    card_areas(area)
        .into_iter()
        .find_map(|(scope, card_area)| contains(card_area, column, row).then_some(scope))
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn render_card(
    frame: &mut Frame,
    area: Rect,
    card: &crate::tui::app::CardData,
    active: bool,
    hovered: bool,
    show_sparklines: bool,
) {
    let surface = if hovered {
        theme::SURFACE_HOVER
    } else {
        theme::SURFACE
    };
    // Card block with border
    let border_style = if active {
        theme::border().fg(theme::ACCENT)
    } else if hovered {
        theme::border().fg(theme::ACCENT_DIM)
    } else {
        theme::border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(theme::card().bg(surface));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 8 {
        return;
    }

    // Layout within card: label, cost, tokens, sparkline
    let constraints = if inner.height >= 5 {
        vec![
            Constraint::Length(1), // label
            Constraint::Length(1), // cost
            Constraint::Length(1), // tokens
            Constraint::Min(1),    // sparkline
        ]
    } else if inner.height >= 3 {
        vec![
            Constraint::Length(1), // label
            Constraint::Length(1), // cost
            Constraint::Length(1), // tokens
        ]
    } else {
        vec![
            Constraint::Length(1), // label
            Constraint::Length(1), // cost
        ]
    };

    let card_areas = Layout::vertical(constraints).split(inner);

    // Label with trend indicator
    let label_style = if active {
        theme::card_label()
            .bg(surface)
            .add_modifier(Modifier::UNDERLINED)
    } else {
        theme::card_label().bg(surface)
    };
    let trend_color = match card.trend.cmp(&0) {
        std::cmp::Ordering::Greater => theme::GREEN,
        std::cmp::Ordering::Less => theme::RED,
        std::cmp::Ordering::Equal => theme::DIM,
    };
    let label = Line::from(vec![
        Span::styled(card.label, label_style),
        Span::styled(
            format!(" {}", card.trend_symbol()),
            ratatui::style::Style::default().fg(trend_color).bg(surface),
        ),
    ]);
    frame.render_widget(label, card_areas[0]);

    // Cost
    let cost_line = Line::from(Span::styled(
        card.cost_str(),
        theme::card_value().bg(surface),
    ));
    frame.render_widget(cost_line, card_areas[1]);

    // Tokens (if space)
    if card_areas.len() >= 3 {
        let tokens_line = Line::from(Span::styled(
            card.tokens_str(),
            theme::card_secondary().bg(surface),
        ));
        frame.render_widget(tokens_line, card_areas[2]);
    }

    // Sparkline (if space and enabled)
    if card_areas.len() >= 4 && !card.sparkline.is_empty() && show_sparklines {
        // Ratatui's Sparkline renders the FIRST N data points (N = widget width).
        // We want to show the most recent data, so slice to the tail.
        let width = card_areas[3].width as usize;
        let data = if card.sparkline.len() > width {
            &card.sparkline[card.sparkline.len() - width..]
        } else {
            &card.sparkline
        };

        let sparkline = Sparkline::default().data(data).style(
            ratatui::style::Style::default()
                .fg(if active {
                    theme::ACCENT
                } else {
                    theme::ACCENT_DIM
                })
                .bg(surface),
        );
        frame.render_widget(sparkline, card_areas[3]);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{card_areas, scope_at};
    use crate::tui::app::Scope;

    #[test]
    fn splits_card_area_into_scope_order() {
        let area = Rect::new(4, 2, 80, 7);
        let cards = card_areas(area);

        assert_eq!(cards[0], (Scope::Today, Rect::new(4, 2, 20, 7)));
        assert_eq!(cards[1], (Scope::Week, Rect::new(24, 2, 20, 7)));
        assert_eq!(cards[2], (Scope::Month, Rect::new(44, 2, 20, 7)));
        assert_eq!(cards[3], (Scope::AllTime, Rect::new(64, 2, 20, 7)));
    }

    #[test]
    fn hit_tests_card_boundaries() {
        let area = Rect::new(0, 1, 80, 7);

        assert_eq!(scope_at(area, 0, 1), Some(Scope::Today));
        assert_eq!(scope_at(area, 19, 7), Some(Scope::Today));
        assert_eq!(scope_at(area, 20, 1), Some(Scope::Week));
        assert_eq!(scope_at(area, 79, 7), Some(Scope::AllTime));
        assert_eq!(scope_at(area, 80, 7), None);
        assert_eq!(scope_at(area, 10, 8), None);
    }
}
