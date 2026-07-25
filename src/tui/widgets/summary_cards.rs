use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Sparkline};
use ratatui::Frame;

use crate::tui::app::{App, Scope};
use crate::tui::theme;

/// Render the visible summary cards: Today, This Week, This Month, All Time.
///
/// Each card shows:
/// - Label (highlighted if it matches the active scope)
/// - Cost (large, bold)
/// - Token count (secondary)
/// - Sparkline (trend)
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let visible_count = app.card_visibility.visible_count();
    if visible_count == 0 {
        return;
    }

    let visible_scopes: Vec<Scope> = Scope::ALL
        .into_iter()
        .filter(|scope| app.card_visibility.is_visible(*scope))
        .collect();
    let constraints = vec![Constraint::Ratio(1, visible_count as u32); visible_count];
    let card_areas = Layout::horizontal(constraints).split(area);
    let show_sparklines = app.config.show_sparklines;
    for (scope, card_area) in visible_scopes.into_iter().zip(card_areas.iter().copied()) {
        render_card(
            frame,
            card_area,
            &app.cards[scope.card_index()],
            scope == app.scope,
            show_sparklines,
        );
    }
}

fn render_card(
    frame: &mut Frame,
    area: Rect,
    card: &crate::tui::app::CardData,
    active: bool,
    show_sparklines: bool,
) {
    // Card block with border
    let border_style = if active {
        theme::border().fg(theme::ACCENT)
    } else {
        theme::border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(theme::card());

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
        theme::card_label().add_modifier(Modifier::UNDERLINED)
    } else {
        theme::card_label()
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
            ratatui::style::Style::default()
                .fg(trend_color)
                .bg(theme::SURFACE),
        ),
    ]);
    frame.render_widget(label, card_areas[0]);

    // Cost
    let cost_line = Line::from(Span::styled(card.cost_str(), theme::card_value()));
    frame.render_widget(cost_line, card_areas[1]);

    // Tokens (if space)
    if card_areas.len() >= 3 {
        let tokens_line = Line::from(Span::styled(card.tokens_str(), theme::card_secondary()));
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
                .bg(theme::SURFACE),
        );
        frame.render_widget(sparkline, card_areas[3]);
    }
}
