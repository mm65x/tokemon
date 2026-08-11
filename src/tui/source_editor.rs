//! Guided editor for read-only, user-defined JSONL sources.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::source::custom::{self, CustomSourceDefinition};
use crate::source::Source;
use crate::tui::event::Event;
use crate::tui::theme;

const FIELD_COUNT: usize = 18;

#[derive(Clone, Copy)]
enum Field {
    Name,
    DisplayName,
    Roots,
    Extension,
    MaxDepth,
    Format,
    Provider,
    ModelPrefix,
    Timestamp,
    Model,
    InputTokens,
    OutputTokens,
    CacheRead,
    CacheWrite,
    Thinking,
    Session,
    Message,
    Request,
}

const FIELDS: [(Field, &str); FIELD_COUNT] = [
    (Field::Name, "Name"),
    (Field::DisplayName, "Display name"),
    (Field::Roots, "Roots (comma-separated)"),
    (Field::Extension, "File extension"),
    (Field::MaxDepth, "Maximum depth"),
    (Field::Format, "Format"),
    (Field::Provider, "Provider override"),
    (Field::ModelPrefix, "Model prefix"),
    (Field::Timestamp, "Timestamp path"),
    (Field::Model, "Model path"),
    (Field::InputTokens, "Input tokens path"),
    (Field::OutputTokens, "Output tokens path"),
    (Field::CacheRead, "Cache read path"),
    (Field::CacheWrite, "Cache write path"),
    (Field::Thinking, "Thinking path"),
    (Field::Session, "Session path"),
    (Field::Message, "Message path"),
    (Field::Request, "Request path"),
];

struct Entry {
    path: Option<PathBuf>,
    definition: CustomSourceDefinition,
}

/// Interactive source-definition editor state.
pub struct State {
    entries: Vec<Entry>,
    selected_source: usize,
    selected_field: usize,
    editing: bool,
    buffer: String,
    status: Option<String>,
    should_quit: bool,
}

impl State {
    /// Load saved definitions, or start with one generic template.
    #[must_use]
    pub fn new() -> Self {
        let mut entries: Vec<Entry> = custom::load_definitions()
            .into_iter()
            .map(|(path, definition)| Entry {
                path: Some(path),
                definition,
            })
            .collect();
        entries.sort_by(|a, b| a.definition.name.cmp(&b.definition.name));
        if entries.is_empty() {
            entries.push(generic_entry());
        }
        Self {
            entries,
            selected_source: 0,
            selected_field: 0,
            editing: false,
            buffer: String::new(),
            status: Some("Press n for a template, t to test, s to save".to_string()),
            should_quit: false,
        }
    }

    /// Whether the editor requested termination.
    #[must_use]
    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Handle one input or timer event.
    pub fn handle_event(&mut self, event: &Event, area: Rect) {
        match event {
            Event::Key(key) => self.handle_key(*key),
            Event::Mouse(mouse) => self.handle_mouse(*mouse, area),
            Event::Tick
            | Event::Render
            | Event::Resize(_, _)
            | Event::DataChanged
            | Event::Warning(_) => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.editing {
            match key.code {
                KeyCode::Enter => self.finish_edit(),
                KeyCode::Esc => {
                    self.editing = false;
                    self.buffer.clear();
                }
                KeyCode::Backspace => {
                    self.buffer.pop();
                }
                KeyCode::Char(c) => self.buffer.push(c),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_field = self.selected_field.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_field = (self.selected_field + 1).min(FIELD_COUNT - 1);
            }
            KeyCode::Tab => self.selected_field = (self.selected_field + 1) % FIELD_COUNT,
            KeyCode::Left | KeyCode::Char('[') => self.select_previous_source(),
            KeyCode::Right | KeyCode::Char(']') => self.select_next_source(),
            KeyCode::Enter => self.begin_edit(),
            KeyCode::Char('n') => self.add_template(false),
            KeyCode::Char('N') => self.add_template(true),
            KeyCode::Char('t') => self.test_current(),
            KeyCode::Char('s') => self.save_current(),
            KeyCode::Char('d') => self.delete_current(),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) {
        if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let (sources, fields) = editor_layout(area);
        if contains(sources, event.column, event.row) {
            let row = event.row.saturating_sub(sources.y) as usize;
            if row < self.entries.len() {
                self.selected_source = row;
                self.selected_field = 0;
            }
        } else if contains(fields, event.column, event.row) {
            let row = event.row.saturating_sub(fields.y) as usize;
            if row < FIELD_COUNT {
                self.selected_field = row;
                self.begin_edit();
            }
        }
    }

    fn current(&self) -> &Entry {
        &self.entries[self.selected_source]
    }

    fn current_mut(&mut self) -> &mut Entry {
        &mut self.entries[self.selected_source]
    }

    fn begin_edit(&mut self) {
        self.buffer = field_value(&self.current().definition, FIELDS[self.selected_field].0);
        self.editing = true;
    }

    fn finish_edit(&mut self) {
        let field = FIELDS[self.selected_field].0;
        let buffer = std::mem::take(&mut self.buffer);
        match set_field(&mut self.current_mut().definition, field, &buffer) {
            Ok(()) => self.status = Some("Edited; press s to validate and save".to_string()),
            Err(error) => self.status = Some(error.to_string()),
        }
        self.editing = false;
    }

    fn add_template(&mut self, nested: bool) {
        let mut entry = generic_entry();
        let suffix = self.entries.len() + 1;
        entry.definition.name = format!("new-source-{suffix}");
        entry.definition.display_name = Some(if nested {
            "Nested usage source".to_string()
        } else {
            "JSONL source".to_string()
        });
        if nested {
            entry.definition.input_tokens = "usage.input".to_string();
            entry.definition.output_tokens = "usage.output".to_string();
            entry.definition.cache_read_tokens = "usage.cached".to_string();
        }
        self.entries.push(entry);
        self.selected_source = self.entries.len() - 1;
        self.selected_field = 0;
        self.status = Some("Template added; edit its fields and press s to save".to_string());
    }

    fn select_previous_source(&mut self) {
        self.selected_source = self.selected_source.saturating_sub(1);
    }

    fn select_next_source(&mut self) {
        self.selected_source = (self.selected_source + 1).min(self.entries.len() - 1);
    }

    fn test_current(&mut self) {
        match custom::CustomSource::from_definition(self.current().definition.clone()) {
            Ok(source) => {
                let files = source.discover_files();
                let records = files
                    .first()
                    .and_then(|file| source.parse_file(file).ok())
                    .map_or(0, |records| records.len());
                self.status = Some(format!(
                    "Valid: {} file(s), {records} record(s) in first file",
                    files.len()
                ));
            }
            Err(error) => self.status = Some(format!("Invalid: {error}")),
        }
    }

    fn save_current(&mut self) {
        match custom::save_definition(&self.current().definition) {
            Ok(path) => {
                self.current_mut().path = Some(path.clone());
                self.status = Some(format!("Saved {}", path.display()));
            }
            Err(error) => self.status = Some(format!("Cannot save: {error}")),
        }
    }

    fn delete_current(&mut self) {
        let name = self.current().definition.name.clone();
        if let Err(error) = custom::delete_definition(&name) {
            self.status = Some(format!("Cannot delete: {error}"));
            return;
        }
        if self.entries.len() > 1 {
            self.entries.remove(self.selected_source);
            self.selected_source = self.selected_source.min(self.entries.len() - 1);
        } else {
            self.entries[0] = generic_entry();
        }
        self.status = Some(format!("Deleted {name}"));
    }
}

/// Run the source editor until the user exits.
pub fn run() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async())
}

async fn run_async() -> anyhow::Result<()> {
    let mut terminal = super::terminal::init()?;
    let mut state = State::new();
    let mut area = Rect::from(terminal.size()?);
    let mut events = super::event::EventHandler::new(
        std::time::Duration::from_secs(2),
        std::time::Duration::from_millis(33),
    );
    events.start();

    while !state.should_quit() {
        let Some(event) = events.next().await else {
            break;
        };
        if let Event::Resize(width, height) = event {
            area = Rect::new(0, 0, width, height);
        }
        state.handle_event(&event, area);
        terminal.draw(|frame| render(frame, &state))?;
    }
    terminal.finish()?;
    Ok(())
}

/// Render the source editor.
pub fn render(frame: &mut Frame, state: &State) {
    let area = frame.area();
    let popup = centered_rect(area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(Span::styled(" Source configuration ", theme::header()))
        .borders(Borders::ALL)
        .border_style(theme::border())
        .style(theme::card());
    frame.render_widget(block, popup);

    let inner = Rect::new(
        popup.x.saturating_add(1),
        popup.y.saturating_add(1),
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(inner);
    let source_items = state.entries.iter().enumerate().map(|(index, entry)| {
        let style = if index == state.selected_source {
            Style::default().fg(theme::FG_BRIGHT).bg(theme::ACCENT_DIM)
        } else {
            theme::text()
        };
        ListItem::new(Line::from(Span::styled(
            format!(
                " {}",
                entry
                    .definition
                    .display_name
                    .as_deref()
                    .unwrap_or(&entry.definition.name)
            ),
            style,
        )))
    });
    let sources = List::new(source_items).block(
        Block::default()
            .title(" Sources ")
            .borders(Borders::ALL)
            .border_style(theme::border()),
    );
    frame.render_widget(sources, columns[0]);

    let entry = state.current();
    let mut lines = Vec::with_capacity(FIELD_COUNT + 4);
    for (index, (field, label)) in FIELDS.iter().enumerate() {
        let value = if state.editing && index == state.selected_field {
            format!("{}|", state.buffer)
        } else {
            field_value(&entry.definition, *field)
        };
        let style = if index == state.selected_field {
            Style::default().fg(theme::FG_BRIGHT).bg(theme::ACCENT_DIM)
        } else {
            theme::text()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {label}: "),
                style.add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(value, style),
        ]));
    }
    let fields = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Definition ")
                .borders(Borders::ALL)
                .border_style(theme::border()),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(fields, columns[1]);

    let footer = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(2),
        inner.width,
        2,
    );
    let status = state.status.as_deref().unwrap_or("");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(format!(" {status}"), theme::text_dim())),
            Line::from(Span::styled(
                " n/N: template  Enter: edit  t: test  s: save  d: delete  [ ]: source  Esc: close",
                theme::status_key(),
            )),
        ]),
        footer,
    );
}

fn generic_entry() -> Entry {
    let definition = CustomSourceDefinition {
        name: "new-source".to_string(),
        display_name: Some("JSONL source".to_string()),
        roots: vec!["~/".to_string()],
        ..CustomSourceDefinition::default()
    };
    Entry {
        path: None,
        definition,
    }
}

fn field_value(definition: &CustomSourceDefinition, field: Field) -> String {
    match field {
        Field::Name => definition.name.clone(),
        Field::DisplayName => definition.display_name.clone().unwrap_or_default(),
        Field::Roots => definition.roots.join(", "),
        Field::Extension => definition.extension.clone(),
        Field::MaxDepth => definition.max_depth.to_string(),
        Field::Format => definition.format.clone(),
        Field::Provider => definition.provider.clone().unwrap_or_default(),
        Field::ModelPrefix => definition.model_prefix.clone(),
        Field::Timestamp => definition.timestamp.clone(),
        Field::Model => definition.model.clone(),
        Field::InputTokens => definition.input_tokens.clone(),
        Field::OutputTokens => definition.output_tokens.clone(),
        Field::CacheRead => definition.cache_read_tokens.clone(),
        Field::CacheWrite => definition.cache_creation_tokens.clone(),
        Field::Thinking => definition.thinking_tokens.clone(),
        Field::Session => definition.session_id.clone(),
        Field::Message => definition.message_id.clone(),
        Field::Request => definition.request_id.clone(),
    }
}

fn set_field(
    definition: &mut CustomSourceDefinition,
    field: Field,
    value: &str,
) -> Result<(), &'static str> {
    let value = value.trim();
    match field {
        Field::Name => definition.name = value.to_string(),
        Field::DisplayName => {
            definition.display_name = (!value.is_empty()).then(|| value.to_string());
        }
        Field::Roots => {
            definition.roots = value
                .split(',')
                .map(str::trim)
                .filter(|root| !root.is_empty())
                .map(str::to_string)
                .collect();
        }
        Field::Extension => definition.extension = value.to_string(),
        Field::MaxDepth => {
            definition.max_depth = value
                .parse()
                .map_err(|_| "maximum depth must be a number")?;
        }
        Field::Format => definition.format = value.to_string(),
        Field::Provider => definition.provider = (!value.is_empty()).then(|| value.to_string()),
        Field::ModelPrefix => definition.model_prefix = value.to_string(),
        Field::Timestamp => definition.timestamp = value.to_string(),
        Field::Model => definition.model = value.to_string(),
        Field::InputTokens => definition.input_tokens = value.to_string(),
        Field::OutputTokens => definition.output_tokens = value.to_string(),
        Field::CacheRead => definition.cache_read_tokens = value.to_string(),
        Field::CacheWrite => definition.cache_creation_tokens = value.to_string(),
        Field::Thinking => definition.thinking_tokens = value.to_string(),
        Field::Session => definition.session_id = value.to_string(),
        Field::Message => definition.message_id = value.to_string(),
        Field::Request => definition.request_id = value.to_string(),
    }
    Ok(())
}

fn editor_layout(area: Rect) -> (Rect, Rect) {
    let popup = centered_rect(area);
    let inner = Rect::new(
        popup.x.saturating_add(1),
        popup.y.saturating_add(1),
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(inner);
    (columns[0], columns[1])
}

fn centered_rect(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(110);
    let height = area.height.saturating_sub(2).min(30);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_have_valid_default_mappings() {
        let generic = generic_entry();
        assert_eq!(generic.definition.format, "jsonl");
        assert_eq!(generic.definition.input_tokens, "usage.input_tokens");
        assert!(custom::CustomSource::from_definition(generic.definition).is_ok());

        let mut nested = generic_entry();
        nested.definition.input_tokens = "usage.input".to_string();
        nested.definition.output_tokens = "usage.output".to_string();
        assert!(custom::CustomSource::from_definition(nested.definition).is_ok());
    }

    #[test]
    fn editing_roots_and_optional_fields_is_trimmed() {
        let mut definition = CustomSourceDefinition::default();
        set_field(&mut definition, Field::Roots, " /tmp/a, ~/b ").unwrap();
        set_field(&mut definition, Field::Provider, " provider ").unwrap();
        assert_eq!(definition.roots, ["/tmp/a", "~/b"]);
        assert_eq!(definition.provider.as_deref(), Some("provider"));

        set_field(&mut definition, Field::Provider, "").unwrap();
        assert!(definition.provider.is_none());
    }
}
