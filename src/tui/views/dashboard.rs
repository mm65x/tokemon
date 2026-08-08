use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::tui::app::{App, HoverTarget, SummaryCardVisibility};
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
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Fill the entire background
    let bg = Block::default().style(theme::text());
    frame.render_widget(bg, area);

    let layout = dashboard_layout(area, app.card_visibility.any_visible());

    // Header
    header::render(frame, layout.header, app);

    // Summary cards (if space and at least one card is visible)
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

/// Calculate the dashboard regions used by both rendering and mouse hit-testing.
#[must_use]
pub(crate) fn dashboard_layout(area: Rect, cards_visible: bool) -> DashboardLayout {
    let card_height = summary_card_height(area.height, cards_visible);
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
pub(crate) fn mouse_action(
    area: Rect,
    visibility: SummaryCardVisibility,
    event: MouseEvent,
) -> Option<MouseAction> {
    let layout = dashboard_layout(area, visibility.any_visible());

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => layout
            .summary_cards
            .and_then(|cards| summary_cards::scope_at(cards, visibility, event.column, event.row))
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

/// Resolve pointer movement to a rendered interactive region.
#[must_use]
pub(crate) fn hover_target(area: Rect, app: &App, event: MouseEvent) -> Option<HoverTarget> {
    let cards_visible = app.card_visibility.any_visible();
    let layout = dashboard_layout(area, cards_visible);

    layout
        .summary_cards
        .and_then(|cards| summary_cards::scope_at(cards, app.card_visibility, event.column, event.row))
        .map(HoverTarget::Card)
        .or_else(|| {
            usage_table::row_at(layout.usage_table, app, event.column, event.row)
                .map(HoverTarget::TableRow)
        })
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

    use super::{dashboard_layout, mouse_action, summary_card_height, MouseAction};
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

    #[test]
    fn dashboard_layout_uses_responsive_card_heights() {
        let visible = SummaryCardVisibility::default();
        let large = dashboard_layout(Rect::new(0, 0, 80, 30), visible.any_visible());
        assert_eq!(large.summary_cards, Some(Rect::new(0, 1, 80, 7)));
        assert_eq!(large.usage_table, Rect::new(0, 8, 80, 21));

        let mut hidden_visibility = SummaryCardVisibility::default();
        for scope in Scope::ALL {
            hidden_visibility.toggle(scope);
        }
        let hidden = dashboard_layout(Rect::new(0, 0, 80, 30), hidden_visibility.any_visible());
        assert_eq!(hidden.summary_cards, None);
        assert_eq!(hidden.usage_table, Rect::new(0, 1, 80, 28));
    }

    #[test]
    fn left_click_on_card_selects_its_scope() {
        let area = Rect::new(0, 0, 80, 30);

        assert_eq!(
            mouse_action(
                area,
                SummaryCardVisibility::default(),
                mouse(MouseEventKind::Down(MouseButton::Left), 45, 2),
            ),
            Some(MouseAction::SelectScope(Scope::Month))
        );
    }

    #[test]
    fn card_click_is_ignored_when_cards_are_not_rendered() {
        let area = Rect::new(0, 0, 80, 30);

        let mut hidden = SummaryCardVisibility::default();
        for scope in Scope::ALL {
            hidden.toggle(scope);
        }
        assert_eq!(
            mouse_action(area, hidden, mouse(MouseEventKind::Down(MouseButton::Left), 5, 2)),
            None
        );
    }

    #[test]
    fn wheel_only_scrolls_over_usage_table() {
        let area = Rect::new(0, 0, 80, 30);

        assert_eq!(
            mouse_action(
                area,
                SummaryCardVisibility::default(),
                mouse(MouseEventKind::ScrollDown, 10, 10),
            ),
            Some(MouseAction::ScrollDown)
        );
        assert_eq!(
            mouse_action(
                area,
                SummaryCardVisibility::default(),
                mouse(MouseEventKind::ScrollUp, 10, 2),
            ),
            None
        );
    }

    #[test]
    fn ignores_other_mouse_events() {
        let area = Rect::new(0, 0, 80, 30);

        assert_eq!(
            mouse_action(
                area,
                SummaryCardVisibility::default(),
                mouse(MouseEventKind::Moved, 10, 10),
            ),
            None
        );
        assert_eq!(
            mouse_action(
                area,
                SummaryCardVisibility::default(),
                mouse(MouseEventKind::Down(MouseButton::Right), 10, 2),
            ),
            None
        );
    }
}
