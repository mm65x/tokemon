use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::config::ColumnConfig;
use crate::display;
use crate::render::{format_cost, format_tokens_short};
use crate::tui::app::App;
use crate::tui::diff::RowKey;
use crate::tui::theme;

/// Render the main usage detail table.
#[allow(clippy::too_many_lines)]
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(
            format!(" {} ", app.scope.label()),
            theme::header(),
        ))
        .style(theme::text());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 20 {
        return;
    }

    // Empty state
    if app.detail_models.is_empty() && app.history_summaries.is_empty() {
        let msg = if app.applied_filter.is_empty() {
            "No usage data found for this period".to_string()
        } else {
            format!("No models matching \"{}\"", app.applied_filter)
        };
        let paragraph = ratatui::widgets::Paragraph::new(ratatui::text::Line::from(Span::styled(
            msg,
            theme::text_dim(),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        let centered_area = Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1);
        frame.render_widget(paragraph, centered_area);
        return;
    }

    // Determine which columns fit, then mask with user config toggles
    let cols = choose_columns(inner.width as usize).mask(&app.config.columns);

    // Build header
    let header_cells: Vec<Cell> = cols
        .headers()
        .into_iter()
        .map(|h| Cell::from(Span::styled(h, theme::header())))
        .collect();
    let header = Row::new(header_cells).height(1);

    // Build data rows
    let mut rows: Vec<Row> = Vec::new();

    if app.show_history && !app.history_summaries.is_empty() {
        // History mode: show per-period summaries with sub-rows
        let today = chrono::Utc::now().date_naive();
        for summary in &app.history_summaries {
            let is_current = summary.date == today
                || (app.scope == crate::tui::app::Scope::Week
                    && summary.date >= crate::tui::app::Scope::Week.since())
                || (app.scope == crate::tui::app::Scope::Month
                    && summary.date >= crate::tui::app::Scope::Month.since());

            let style = if is_current {
                theme::text_bold()
            } else {
                theme::text_dim()
            };

            // Period summary row
            let total = summary.total_input
                + summary.total_output
                + summary.total_cache()
                + summary.total_thinking;
            let period_cells = cols.build_row(
                &summary.label,
                "",
                "",
                summary.total_requests,
                summary.total_input,
                summary.total_output,
                total,
                summary.total_cost,
                style,
                is_current,
                0.0, // no per-cell highlight in history mode
            );
            rows.push(Row::new(period_cells).height(1));

            // Model sub-rows under each period header
            {
                for mu in &summary.models {
                    let model_total = mu.total_tokens();
                    let sub_cells = cols.build_row(
                        &format!("  {}", display::display_model(&mu.model)),
                        display::infer_api_provider(mu.effective_raw_model()),
                        "",
                        mu.request_count,
                        mu.input_tokens,
                        mu.output_tokens,
                        model_total,
                        mu.cost_usd,
                        style,
                        is_current,
                        0.0, // no per-cell highlight in history mode
                    );
                    rows.push(Row::new(sub_cells).height(1));
                }
            }
        }
    } else {
        // Normal mode: flat list for the scope
        for mu in &app.detail_models {
            let total = mu.total_tokens();

            // Columns depend on group-by mode.
            // Use raw_model for API provider inference (retains routing
            // prefix like "vertexai."), normalized model for display name.
            let (name_col, api_col, client_col) = match app.group_by {
                crate::types::GroupBy::Model => (
                    display::display_model(&mu.model),
                    display::infer_api_provider(mu.effective_raw_model()).to_string(),
                    String::new(),
                ),
                crate::types::GroupBy::ModelClient => (
                    display::display_model(&mu.model),
                    display::infer_api_provider(mu.effective_raw_model()).to_string(),
                    display::display_client(&mu.provider).into_owned(),
                ),
                crate::types::GroupBy::Client => (
                    display::display_client(&mu.provider).into_owned(),
                    String::new(),
                    String::new(),
                ),
            };

            // Look up per-row highlight intensity
            let row_key = RowKey::from(mu);
            let intensity = app.highlight_intensity(&row_key);

            let cells = cols.build_row(
                &name_col,
                &api_col,
                &client_col,
                mu.request_count,
                mu.input_tokens,
                mu.output_tokens,
                total,
                mu.cost_usd,
                theme::text(),
                true,
                intensity,
            );
            rows.push(Row::new(cells).height(1));
        }
    }

    // Total row
    let total_cells = cols.build_total_row(
        app.detail_total_requests,
        app.detail_total_tokens,
        app.detail_total_cost,
    );
    rows.push(Row::new(total_cells).height(1));

    let row_count = rows.len();
    let table = Table::new(rows, cols.widths())
        .header(header)
        .row_highlight_style(theme::text().add_modifier(Modifier::REVERSED));

    // Apply scroll offset via TableState
    let offset = app.scroll_offset as usize;
    let mut table_state =
        TableState::default().with_offset(offset.min(row_count.saturating_sub(1)));
    frame.render_stateful_widget(table, inner, &mut table_state);

    // Clamp scroll_offset if it exceeds available rows (borrow after rendering)
    // This is a visual-only clamp; actual state clamping happens in app.rs
}

// ── Column management ─────────────────────────────────────────────────────

/// Which columns to display, based on available width.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ColumnSet {
    show_api: bool,
    show_client: bool,
    show_input: bool,
    show_output: bool,
    show_requests: bool,
}

impl ColumnSet {
    /// Mask responsive column choices against user config toggles.
    /// A column is only shown if both the responsive set AND config allow it.
    fn mask(self, cfg: &ColumnConfig) -> Self {
        Self {
            show_api: self.show_api && cfg.api_provider,
            show_client: self.show_client && cfg.client,
            show_requests: self.show_requests,
            show_input: self.show_input && cfg.input,
            show_output: self.show_output && cfg.output,
        }
    }

    fn headers(self) -> Vec<String> {
        let mut h = vec!["Model".to_string()];
        if self.show_api {
            h.push("API".to_string());
        }
        if self.show_client {
            h.push("Client".to_string());
        }
        if self.show_requests {
            h.push("Reqs".to_string());
        }
        if self.show_input {
            h.push("Input".to_string());
        }
        if self.show_output {
            h.push("Output".to_string());
        }
        h.push("Total".to_string());
        h.push("Cost".to_string());
        h
    }

    fn widths(self) -> Vec<Constraint> {
        let mut w: Vec<Constraint> = vec![Constraint::Min(12)]; // Model
        if self.show_api {
            w.push(Constraint::Length(12));
        }
        if self.show_client {
            w.push(Constraint::Length(14));
        }
        if self.show_requests {
            w.push(Constraint::Length(6));
        }
        if self.show_input {
            w.push(Constraint::Length(8));
        }
        if self.show_output {
            w.push(Constraint::Length(8));
        }
        w.push(Constraint::Length(8)); // Total
        w.push(Constraint::Length(10)); // Cost
        w
    }

    #[allow(clippy::too_many_arguments)]
    fn build_row(
        self,
        col0: &str,
        col1: &str,
        col2: &str,
        requests: u64,
        input: u64,
        output: u64,
        total: u64,
        cost: f64,
        base_style: Style,
        use_color: bool,
        highlight_intensity: f64,
    ) -> Vec<Cell<'static>> {
        let mut cells: Vec<Cell> = Vec::new();

        // Name / label columns — apply bold when highlighted, but not green
        let name_style = if highlight_intensity > 0.4 {
            base_style.add_modifier(Modifier::BOLD)
        } else {
            base_style
        };

        cells.push(Cell::from(Span::styled(col0.to_string(), name_style)));

        if self.show_api {
            cells.push(Cell::from(Span::styled(col1.to_string(), name_style)));
        }
        if self.show_client {
            cells.push(Cell::from(Span::styled(col2.to_string(), name_style)));
        }

        // Requests column
        if self.show_requests {
            let s = if requests > 0 {
                format_tokens_short(requests)
            } else {
                String::new()
            };
            let style = apply_highlight(base_style, highlight_intensity);
            cells.push(Cell::from(Span::styled(s, style)));
        }

        // Token and cost columns get the green highlight effect
        if self.show_input {
            let s = format_tokens_short(input);
            let normal_style = if use_color {
                theme::tokens_style(input)
            } else {
                base_style
            };
            let style = apply_highlight(normal_style, highlight_intensity);
            cells.push(Cell::from(Span::styled(s, style)));
        }
        if self.show_output {
            let s = format_tokens_short(output);
            let normal_style = if use_color {
                theme::tokens_style(output)
            } else {
                base_style
            };
            let style = apply_highlight(normal_style, highlight_intensity);
            cells.push(Cell::from(Span::styled(s, style)));
        }

        let total_s = format_tokens_short(total);
        let normal_total = if use_color {
            theme::tokens_style(total)
        } else {
            base_style
        };
        let total_style = apply_highlight(normal_total, highlight_intensity);
        cells.push(Cell::from(Span::styled(total_s, total_style)));

        let cost_s = format_cost(cost);
        let normal_cost = if use_color {
            theme::cost_style(cost)
        } else {
            base_style
        };
        let cost_style = apply_highlight(normal_cost, highlight_intensity);
        cells.push(Cell::from(Span::styled(cost_s, cost_style)));

        cells
    }

    fn build_total_row(
        self,
        total_requests: u64,
        total_tokens: u64,
        total_cost: f64,
    ) -> Vec<Cell<'static>> {
        let style = theme::total_row();
        let mut cells: Vec<Cell> = vec![Cell::from(Span::styled("TOTAL", style))];
        if self.show_api {
            cells.push(Cell::from(Span::styled("", style)));
        }
        if self.show_client {
            cells.push(Cell::from(Span::styled("", style)));
        }
        if self.show_requests {
            cells.push(Cell::from(Span::styled(
                format_tokens_short(total_requests),
                style,
            )));
        }
        if self.show_input {
            cells.push(Cell::from(Span::styled("", style)));
        }
        if self.show_output {
            cells.push(Cell::from(Span::styled("", style)));
        }
        cells.push(Cell::from(Span::styled(
            format_tokens_short(total_tokens),
            style,
        )));
        cells.push(Cell::from(Span::styled(format_cost(total_cost), style)));
        cells
    }
}

/// Choose which columns to display based on terminal width.
fn choose_columns(width: usize) -> ColumnSet {
    if width >= 86 {
        ColumnSet {
            show_api: true,
            show_client: true,
            show_requests: true,
            show_input: true,
            show_output: true,
        }
    } else if width >= 70 {
        ColumnSet {
            show_api: true,
            show_client: false,
            show_requests: true,
            show_input: true,
            show_output: true,
        }
    } else if width >= 56 {
        ColumnSet {
            show_api: false,
            show_client: false,
            show_requests: true,
            show_input: true,
            show_output: true,
        }
    } else if width >= 42 {
        ColumnSet {
            show_api: false,
            show_client: false,
            show_requests: true,
            show_input: false,
            show_output: false,
        }
    } else {
        ColumnSet {
            show_api: false,
            show_client: false,
            show_requests: false,
            show_input: false,
            show_output: false,
        }
    }
}

/// Apply the green highlight effect to a cell style based on intensity.
/// Returns the original style if intensity is 0.
fn apply_highlight(normal: Style, intensity: f64) -> Style {
    if intensity <= 0.0 {
        return normal;
    }
    // Extract the foreground colour from the normal style, defaulting to FG
    let normal_fg = normal.fg.unwrap_or(theme::FG);
    theme::highlight_cell(intensity, normal_fg)
}
