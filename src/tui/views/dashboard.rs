use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::views::{help, settings};
use crate::tui::widgets::{header, status_bar, summary_cards, usage_table};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseAction {
    SelectScope(crate::tui::app::Scope),
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DashboardLayout {
    header: Rect,
    summary_cards: Option<Rect>,
    pub usage_table: Rect,
    status_bar: Rect,
}

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

    let layout = dashboard_layout(area);

    // Header
    header::render(frame, layout.header, app);

    // Summary cards (if space)
    if let Some(cards_area) = layout.summary_cards {
        summary_cards::render(frame, cards_area, app);
    }

    // Usage table
    usage_table::render(frame, layout.usage_table, app);

    // Status bar
    status_bar::render(frame, layout.status_bar, app);

    // Overlays (rendered on top of everything)
    if app.show_help {
        help::render(frame);
    }
    if app.show_settings {
        settings::render(frame, app);
    }
}

/// Calculate the dashboard regions used by both rendering and mouse hit-testing.
#[must_use]
pub(crate) fn dashboard_layout(area: Rect) -> DashboardLayout {
    let card_height = if area.height >= 30 {
        7
    } else if area.height >= 20 {
        5
    } else {
        0
    };

    let mut constraints = vec![Constraint::Length(1)];
    if card_height > 0 {
        constraints.push(Constraint::Length(card_height));
    }
    constraints.push(Constraint::Min(5));
    constraints.push(Constraint::Length(1));

    let areas = Layout::vertical(constraints).split(area);
    let mut index = 0;
    let header = areas[index];
    index += 1;
    let summary_cards = if card_height > 0 {
        let cards = areas[index];
        index += 1;
        Some(cards)
    } else {
        None
    };

    DashboardLayout {
        header,
        summary_cards,
        usage_table: areas[index],
        status_bar: areas[index + 1],
    }
}

/// Translate a raw mouse event into a dashboard action.
#[must_use]
pub(crate) fn mouse_action(area: Rect, event: MouseEvent) -> Option<MouseAction> {
    let layout = dashboard_layout(area);

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => layout
            .summary_cards
            .and_then(|cards| summary_cards::scope_at(cards, event.column, event.row))
            .map(MouseAction::SelectScope),
        MouseEventKind::ScrollUp if contains(layout.usage_table, event.column, event.row) => {
            Some(MouseAction::ScrollUp)
        }
        MouseEventKind::ScrollDown if contains(layout.usage_table, event.column, event.row) => {
            Some(MouseAction::ScrollDown)
        }
        _ => None,
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    use super::{dashboard_layout, mouse_action, MouseAction};
    use crate::tui::app::Scope;

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn dashboard_layout_uses_responsive_card_heights() {
        let large = dashboard_layout(Rect::new(0, 0, 80, 30));
        assert_eq!(large.summary_cards, Some(Rect::new(0, 1, 80, 7)));
        assert_eq!(large.usage_table, Rect::new(0, 8, 80, 21));

        let medium = dashboard_layout(Rect::new(0, 0, 80, 20));
        assert_eq!(medium.summary_cards, Some(Rect::new(0, 1, 80, 5)));
        assert_eq!(medium.usage_table, Rect::new(0, 6, 80, 13));

        let small = dashboard_layout(Rect::new(0, 0, 80, 19));
        assert_eq!(small.summary_cards, None);
        assert_eq!(small.usage_table, Rect::new(0, 1, 80, 17));
    }

    #[test]
    fn left_click_on_card_selects_its_scope() {
        let area = Rect::new(0, 0, 80, 30);

        assert_eq!(
            mouse_action(area, mouse(MouseEventKind::Down(MouseButton::Left), 45, 2)),
            Some(MouseAction::SelectScope(Scope::Month))
        );
    }

    #[test]
    fn card_click_is_ignored_when_cards_are_not_rendered() {
        let area = Rect::new(0, 0, 80, 19);

        assert_eq!(
            mouse_action(area, mouse(MouseEventKind::Down(MouseButton::Left), 5, 2)),
            None
        );
    }

    #[test]
    fn wheel_only_scrolls_over_usage_table() {
        let area = Rect::new(0, 0, 80, 30);

        assert_eq!(
            mouse_action(area, mouse(MouseEventKind::ScrollDown, 10, 10)),
            Some(MouseAction::ScrollDown)
        );
        assert_eq!(
            mouse_action(area, mouse(MouseEventKind::ScrollUp, 10, 2)),
            None
        );
    }

    #[test]
    fn ignores_other_mouse_events() {
        let area = Rect::new(0, 0, 80, 30);

        assert_eq!(
            mouse_action(area, mouse(MouseEventKind::Moved, 10, 10)),
            None
        );
        assert_eq!(
            mouse_action(area, mouse(MouseEventKind::Down(MouseButton::Right), 10, 2)),
            None
        );
    }
}
