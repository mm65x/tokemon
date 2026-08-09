use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::config::Config;
use crate::tui::app::App;
use crate::tui::settings_state::SettingField;
use crate::tui::theme;

/// Actions that a pointer can perform in the settings overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseAction {
    SelectField(usize),
    ActivateField(usize),
    ScrollUp,
    ScrollDown,
    ReviewSave,
    ConfirmSave,
    CancelSave,
    Discard,
    Close,
}

#[derive(Debug, Clone, Copy)]
struct SettingsLayout {
    content: Rect,
    footer: Rect,
    scroll_offset: usize,
}

/// Render the settings overlay as a centered popup.
#[allow(clippy::too_many_lines)]
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let state = &app.settings_state;

    // Size: wide enough for labels + values, tall enough for all fields + sections + footer
    let popup_width = area.width.min(80);
    let popup_height = area.height.min(32);

    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Title with unsaved indicator
    let title = if state.confirming_save {
        " Save changes? ".to_string()
    } else if state.unsaved {
        " Settings [modified] ".to_string()
    } else {
        " Settings ".to_string()
    };

    let block = Block::default()
        .title(Span::styled(title, theme::header()))
        .borders(Borders::ALL)
        .border_style(theme::border().fg(if state.unsaved {
            theme::YELLOW
        } else {
            theme::ACCENT
        }))
        .style(theme::card());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Build the content lines, tracking which line index each field maps to.
    let mut lines: Vec<Line> = Vec::with_capacity(SettingField::COUNT + 10);
    let mut field_line_indices: Vec<usize> = Vec::with_capacity(SettingField::COUNT);

    let config_path = Config::config_path();
    let source_label = if config_path.exists() {
        "saved configuration"
    } else {
        "built-in defaults (file will be created on save)"
    };
    lines.push(Line::from(Span::styled(
        format!("  Source: {source_label}"),
        theme::text_dim(),
    )));
    lines.push(Line::from(Span::styled(
        format!("  Path: {}", config_path.display()),
        theme::text_dim(),
    )));
    lines.push(Line::from(""));

    for (idx, field) in SettingField::ALL.iter().enumerate() {
        // Section header
        if let Some(header) = field.section_header() {
            if idx > 0 {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  {header}"),
                theme::header(),
            )));
        }

        field_line_indices.push(lines.len()); // record the line index for this field

        let is_selected = idx == state.selected;
        let value_str = if state.editing && is_selected {
            // Show edit buffer with cursor
            format!("{}|", state.edit_buffer)
        } else {
            field.display_value(&state.draft)
        };

        // Format the value display based on field type
        let value_display = if field.is_bool() {
            let on = value_str == "Yes";
            if on { "[x]" } else { "[ ]" }.to_string()
        } else if field.is_enum() && is_selected {
            format!("<  {value_str}  >")
        } else {
            value_str
        };

        let label = field.label();

        // Calculate padding for right-aligned value
        let inner_width = inner.width as usize;
        let label_part = format!("  {label}");
        let padding = inner_width
            .saturating_sub(label_part.len())
            .saturating_sub(value_display.len())
            .saturating_sub(2); // right margin

        let line = if is_selected {
            let bg = theme::ACCENT_DIM;
            Line::from(vec![
                Span::styled(
                    label_part,
                    ratatui::style::Style::default()
                        .fg(theme::FG_BRIGHT)
                        .bg(bg)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                Span::styled(" ".repeat(padding), ratatui::style::Style::default().bg(bg)),
                Span::styled(
                    value_display,
                    ratatui::style::Style::default()
                        .fg(if state.editing {
                            theme::YELLOW
                        } else {
                            theme::FG_BRIGHT
                        })
                        .bg(bg)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                Span::styled("  ", ratatui::style::Style::default().bg(bg)),
            ])
        } else {
            Line::from(vec![
                Span::styled(label_part, theme::text_dim()),
                Span::styled(" ".repeat(padding), theme::card()),
                Span::styled(value_display, theme::text()),
                Span::styled("  ", theme::card()),
            ])
        };

        lines.push(line);
    }

    // Flash message
    if let Some((msg, _)) = &state.flash_message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {msg}"),
            ratatui::style::Style::default()
                .fg(theme::GREEN)
                .bg(theme::SURFACE)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )));
    }

    // Split inner into scrollable content area and fixed footer
    let footer_height: u16 = if state.unsaved { 3 } else { 2 };
    let content_height = inner.height.saturating_sub(footer_height);
    let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);
    let footer_area = Rect::new(
        inner.x,
        inner.y + content_height,
        inner.width,
        footer_height,
    );

    // Determine scroll for content area
    let visible_height = content_height as usize;
    let selected_line_idx = field_line_indices.get(state.selected).copied().unwrap_or(0);

    let scroll_offset = if selected_line_idx >= visible_height {
        selected_line_idx.saturating_sub(visible_height / 2)
    } else {
        0
    };

    #[allow(clippy::cast_possible_truncation)]
    let paragraph = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
    frame.render_widget(paragraph, content_area);

    // Fixed footer — always visible
    let mut footer_lines: Vec<Line> = Vec::new();

    // Unsaved changes prompt
    if state.confirming_save {
        footer_lines.push(Line::from(vec![
            Span::styled("  Y/Enter", theme::status_key()),
            Span::styled(": Confirm save  ", theme::card_secondary()),
            Span::styled("N/Esc", theme::status_key()),
            Span::styled(": Cancel", theme::card_secondary()),
        ]));
    } else if state.unsaved {
        footer_lines.push(Line::from(vec![
            Span::styled("  W", theme::status_key()),
            Span::styled(
                ": Review save  ",
                ratatui::style::Style::default()
                    .fg(theme::YELLOW)
                    .bg(theme::SURFACE),
            ),
            Span::styled("Esc", theme::status_key()),
            Span::styled(": Discard", theme::card_secondary()),
        ]));
    }

    let nav_line = if state.editing {
        Line::from(vec![
            Span::styled("  Enter", theme::status_key()),
            Span::styled(": Apply  ", theme::card_secondary()),
            Span::styled("Esc", theme::status_key()),
            Span::styled(": Cancel", theme::card_secondary()),
        ])
    } else {
        Line::from(vec![
            Span::styled("  \u{2191}\u{2193}", theme::status_key()),
            Span::styled(": Navigate  ", theme::card_secondary()),
            Span::styled("Enter", theme::status_key()),
            Span::styled(": Edit/Toggle  ", theme::card_secondary()),
            Span::styled("*", theme::text_dim()),
            Span::styled(" Restart required", theme::text_dim()),
        ])
    };
    footer_lines.push(nav_line);

    // Save/close hint (when not showing unsaved prompt)
    if !state.unsaved {
        footer_lines.push(Line::from(vec![
            Span::styled("  Esc", theme::status_key()),
            Span::styled(": Close", theme::card_secondary()),
        ]));
    }

    let footer_paragraph = Paragraph::new(footer_lines);
    frame.render_widget(footer_paragraph, footer_area);
}

/// Translate a pointer event into a settings action using the same geometry as
/// the renderer. Pointer movement selects a field for visual feedback, while a
/// left click activates it just like Enter/Space.
#[must_use]
pub(crate) fn mouse_action(area: Rect, app: &App, event: MouseEvent) -> Option<MouseAction> {
    let layout = settings_layout(area, app);

    if let Some(field) = field_at(layout, event.column, event.row) {
        return match event.kind {
            MouseEventKind::Moved => Some(MouseAction::SelectField(field)),
            MouseEventKind::Down(MouseButton::Left) => Some(MouseAction::ActivateField(field)),
            MouseEventKind::ScrollUp => Some(MouseAction::ScrollUp),
            MouseEventKind::ScrollDown => Some(MouseAction::ScrollDown),
            _ => None,
        };
    }

    if let Some(action) = footer_action(
        layout,
        app.settings_state.unsaved,
        app.settings_state.confirming_save,
        event,
    ) {
        return Some(action);
    }

    if contains(layout.content, event.column, event.row) {
        return match event.kind {
            MouseEventKind::ScrollUp => Some(MouseAction::ScrollUp),
            MouseEventKind::ScrollDown => Some(MouseAction::ScrollDown),
            _ => None,
        };
    }

    None
}

fn settings_layout(area: Rect, app: &App) -> SettingsLayout {
    let popup_width = area.width.min(80);
    let popup_height = area.height.min(32);
    let popup_area = centered_rect(popup_width, popup_height, area);
    let inner = Rect::new(
        popup_area.x.saturating_add(1),
        popup_area.y.saturating_add(1),
        popup_area.width.saturating_sub(2),
        popup_area.height.saturating_sub(2),
    );
    let footer_height: u16 = if app.settings_state.unsaved || app.settings_state.confirming_save {
        3
    } else {
        2
    };
    let content_height = inner.height.saturating_sub(footer_height);
    let content = Rect::new(inner.x, inner.y, inner.width, content_height);
    let footer = Rect::new(
        inner.x,
        inner.y.saturating_add(content_height),
        inner.width,
        footer_height,
    );
    let selected_line = field_line_indices()
        .get(app.settings_state.selected)
        .copied()
        .unwrap_or_default();
    let scroll_offset = if selected_line >= content_height as usize {
        selected_line.saturating_sub(content_height as usize / 2)
    } else {
        0
    };

    SettingsLayout {
        content,
        footer,
        scroll_offset,
    }
}

fn field_line_indices() -> Vec<usize> {
    let mut lines = 3; // source, path, blank
    let mut indices = Vec::with_capacity(SettingField::COUNT);
    for (idx, field) in SettingField::ALL.iter().enumerate() {
        if field.section_header().is_some() {
            if idx > 0 {
                lines += 1;
            }
            lines += 1;
        }
        indices.push(lines);
        lines += 1;
    }
    indices
}

fn field_at(layout: SettingsLayout, column: u16, row: u16) -> Option<usize> {
    if !contains(layout.content, column, row) {
        return None;
    }
    let line = layout.scroll_offset + row.saturating_sub(layout.content.y) as usize;
    field_line_indices().iter().position(|index| *index == line)
}

fn footer_action(
    layout: SettingsLayout,
    unsaved: bool,
    confirming_save: bool,
    event: MouseEvent,
) -> Option<MouseAction> {
    if contains(layout.content, event.column, event.row)
        || !contains(layout.footer, event.column, event.row)
    {
        return None;
    }
    let row = event.row.saturating_sub(layout.footer.y);
    let midpoint = layout.footer.x + layout.footer.width / 2;
    if row == 0 && matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        if confirming_save {
            return Some(if event.column < midpoint {
                MouseAction::ConfirmSave
            } else {
                MouseAction::CancelSave
            });
        }
        if unsaved {
            return Some(if event.column < midpoint {
                MouseAction::ReviewSave
            } else {
                MouseAction::Discard
            });
        }
    }
    if !unsaved
        && !confirming_save
        && row == layout.footer.height.saturating_sub(1)
        && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
    {
        return Some(MouseAction::Close);
    }
    None
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let [popup_area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(
            Layout::vertical([Constraint::Length(height)])
                .flex(Flex::Center)
                .areas::<1>(area)[0],
        );
    popup_area
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    use super::{field_at, field_line_indices, footer_action, MouseAction, SettingsLayout};

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn visible_field_rows_resolve_and_headers_do_not() {
        let layout = SettingsLayout {
            content: Rect::new(5, 4, 30, 6),
            footer: Rect::new(5, 10, 30, 2),
            scroll_offset: field_line_indices()[0],
        };

        assert_eq!(field_at(layout, 6, 4), Some(0));
        assert_eq!(field_at(layout, 6, 5), None);
        assert_eq!(field_at(layout, 4, 4), None);
    }

    #[test]
    fn scroll_offset_resolves_later_fields() {
        let indices = field_line_indices();
        let layout = SettingsLayout {
            content: Rect::new(0, 0, 20, 4),
            footer: Rect::new(0, 4, 20, 2),
            scroll_offset: indices[indices.len() - 1],
        };

        assert_eq!(field_at(layout, 10, 0), Some(indices.len() - 1));
        assert_eq!(field_at(layout, 21, 0), None);
    }

    #[test]
    fn footer_clicks_map_to_save_and_close_actions() {
        let layout = SettingsLayout {
            content: Rect::new(0, 0, 20, 4),
            footer: Rect::new(0, 4, 20, 3),
            scroll_offset: 0,
        };
        assert_eq!(
            footer_action(
                layout,
                true,
                false,
                mouse(MouseEventKind::Down(MouseButton::Left), 2, 4)
            ),
            Some(MouseAction::ReviewSave)
        );
        assert_eq!(
            footer_action(
                layout,
                true,
                false,
                mouse(MouseEventKind::Down(MouseButton::Left), 18, 4)
            ),
            Some(MouseAction::Discard)
        );

        let clean = SettingsLayout {
            footer: Rect::new(0, 4, 20, 2),
            ..layout
        };
        assert_eq!(
            footer_action(
                clean,
                false,
                false,
                mouse(MouseEventKind::Down(MouseButton::Left), 10, 5)
            ),
            Some(MouseAction::Close)
        );
    }
}
