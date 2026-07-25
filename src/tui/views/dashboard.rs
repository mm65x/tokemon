use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::views::{help, settings};
use crate::tui::widgets::{header, status_bar, summary_cards, usage_table};

/// Render the complete dashboard view.
///
/// Layout:
/// ```text
/// ┌────────────── header (1 line) ──────────────┐
/// ├──────────── summary cards (7 lines) ────────┤
/// ├────────── usage table (flexible) ───────────┤
/// ├────────────── status bar (1 line) ──────────┤
/// └─────────────────────────────────────────────┘
/// ```
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Fill the entire background
    let bg = Block::default().style(theme::text());
    frame.render_widget(bg, area);

    let card_height = summary_card_height(area.height, app.card_visibility.any_visible());

    let mut constraints = vec![
        Constraint::Length(1), // header
    ];

    if card_height > 0 {
        constraints.push(Constraint::Length(card_height)); // summary cards
    }

    constraints.push(Constraint::Min(5)); // usage table
    constraints.push(Constraint::Length(1)); // status bar

    let layout = Layout::vertical(constraints).split(area);

    let mut idx = 0;

    // Header
    header::render(frame, layout[idx], app);
    idx += 1;

    // Summary cards (if space)
    if card_height > 0 {
        summary_cards::render(frame, layout[idx], app);
        idx += 1;
    }

    // Usage table
    usage_table::render(frame, layout[idx], app);
    idx += 1;

    // Status bar
    status_bar::render(frame, layout[idx], app);

    // Overlays (rendered on top of everything)
    if app.show_help {
        help::render(frame);
    }
    if app.show_settings {
        settings::render(frame, app);
    }
}

#[must_use]
const fn summary_card_height(terminal_height: u16, has_visible_cards: bool) -> u16 {
    if !has_visible_cards {
        0
    } else if terminal_height >= 30 {
        7
    } else if terminal_height >= 20 {
        5
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::summary_card_height;

    #[test]
    fn hidden_cards_reclaim_the_layout_space() {
        assert_eq!(summary_card_height(40, false), 0);
        assert_eq!(summary_card_height(25, false), 0);
    }

    #[test]
    fn visible_cards_keep_responsive_height() {
        assert_eq!(summary_card_height(30, true), 7);
        assert_eq!(summary_card_height(20, true), 5);
        assert_eq!(summary_card_height(19, true), 0);
    }
}
