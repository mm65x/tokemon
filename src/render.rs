use std::io::IsTerminal;

use tabled::builder::Builder;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Modify, Style};
use tabled::{Table, Tabled};

use crate::display;
use crate::types::{Report, SessionReport};

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// Whether to emit ANSI color codes. Respects NO_COLOR and non-TTY pipes.
#[must_use]
fn use_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn ansi(code: &str, s: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn bold(s: &str, c: bool) -> String {
    ansi("1", s, c)
}
fn dim(s: &str, c: bool) -> String {
    ansi("2", s, c)
}
fn cyan_bold(s: &str, c: bool) -> String {
    ansi("1;36", s, c)
}
fn green(s: &str, c: bool) -> String {
    ansi("32", s, c)
}
fn yellow(s: &str, c: bool) -> String {
    ansi("33", s, c)
}
fn red(s: &str, c: bool) -> String {
    ansi("31", s, c)
}

/// Format a cost value with color coding.
fn format_cost_styled(cost: f64, color: bool) -> String {
    let s = format_cost(cost);
    if !color {
        return s;
    }
    if cost == 0.0 {
        dim(&s, true)
    } else if cost < 1.0 {
        green(&s, true)
    } else if cost < 10.0 {
        yellow(&s, true)
    } else {
        red(&s, true)
    }
}

/// Format a token count with dim styling for zeros.
fn format_tokens_styled(n: u64, color: bool) -> String {
    let s = format_tokens(n);
    if color && n == 0 {
        dim(&s, true)
    } else {
        s
    }
}

/// Apply bold to every element in a row.
fn bold_row(row: &mut [String], color: bool) {
    if !color {
        return;
    }
    for cell in row.iter_mut() {
        if !cell.is_empty() {
            *cell = bold(cell, true);
        }
    }
}

/// Style each element of the header row.
fn style_header(header: &mut [String], color: bool) {
    if !color {
        return;
    }
    for cell in header.iter_mut() {
        *cell = cyan_bold(cell, true);
    }
}

// ---------------------------------------------------------------------------
// Responsive columns
// ---------------------------------------------------------------------------

/// Terminal width in visible columns.
#[must_use]
fn terminal_width() -> usize {
    terminal_size::terminal_size().map_or(120, |(w, _)| w.0 as usize)
}

/// Visible width of a string, ignoring ANSI escape codes.
#[must_use]
fn display_width(s: &str) -> usize {
    let mut w = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else if c == '\x1b' {
            in_escape = true;
        } else {
            w += 1;
        }
    }
    w
}

// ---------------------------------------------------------------------------
// Table printing
// ---------------------------------------------------------------------------

#[derive(Tabled)]
struct DiscoverRow {
    #[tabled(rename = "Provider")]
    provider: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Data Dir")]
    data_dir: String,
    #[tabled(rename = "Files")]
    files: String,
}

pub fn print_table(report: &Report, breakdown: bool, col_cfg: &crate::config::ColumnConfig) {
    if report.summaries.is_empty() {
        println!("No usage data found.");
        return;
    }

    if breakdown {
        print_breakdown_table(report, col_cfg);
    } else {
        print_compact_table(report, col_cfg);
    }
}

#[derive(Clone, Copy)]
#[allow(clippy::similar_names)]
struct BreakdownCols {
    show_in: bool,
    show_out: bool,
    show_cw: bool,
    show_cr: bool,
    show_client: bool,
    show_api: bool,
}

impl BreakdownCols {
    /// Mask responsive column choices against user config toggles.
    /// A column is only shown if both the responsive set AND config allow it.
    fn mask(self, cfg: &crate::config::ColumnConfig) -> Self {
        Self {
            show_in: self.show_in && cfg.input,
            show_out: self.show_out && cfg.output,
            show_cw: self.show_cw && cfg.cache_write,
            show_cr: self.show_cr && cfg.cache_read,
            show_client: self.show_client && cfg.client,
            show_api: self.show_api && cfg.api_provider,
        }
    }
}

fn print_breakdown_table(report: &Report, col_cfg: &crate::config::ColumnConfig) {
    let color = use_color();
    let width = terminal_width();

    // Try column sets from most to fewest until the table fits.
    // Hide priority: client, api_provider, cache_write, cache_read, in+out
    let column_sets = [
        BreakdownCols {
            show_in: true,
            show_out: true,
            show_cw: true,
            show_cr: true,
            show_client: true,
            show_api: true,
        },
        BreakdownCols {
            show_in: true,
            show_out: true,
            show_cw: true,
            show_cr: true,
            show_client: false,
            show_api: true,
        },
        BreakdownCols {
            show_in: true,
            show_out: true,
            show_cw: true,
            show_cr: true,
            show_client: false,
            show_api: false,
        },
        BreakdownCols {
            show_in: true,
            show_out: true,
            show_cw: false,
            show_cr: true,
            show_client: false,
            show_api: false,
        },
        BreakdownCols {
            show_in: true,
            show_out: true,
            show_cw: false,
            show_cr: false,
            show_client: false,
            show_api: false,
        },
        BreakdownCols {
            show_in: false,
            show_out: false,
            show_cw: false,
            show_cr: false,
            show_client: false,
            show_api: false,
        },
    ];

    for cols in &column_sets {
        let masked = cols.mask(col_cfg);
        let table = render_breakdown(report, color, masked);
        let first_line = table.lines().next().unwrap_or("");
        if display_width(first_line) <= width || (!masked.show_in && !masked.show_out) {
            println!("{table}");
            return;
        }
    }
}

#[allow(clippy::too_many_lines)]
fn render_breakdown(report: &Report, color: bool, cols: BreakdownCols) -> String {
    let BreakdownCols {
        show_in,
        show_out,
        show_cw,
        show_cr,
        show_client,
        show_api,
    } = cols;
    let mut header: Vec<String> = vec!["Date".into(), "Model".into()];
    if show_api {
        header.push("API Provider".into());
    }
    if show_client {
        header.push("Client".into());
    }
    if show_in {
        header.push("Input".into());
    }
    if show_out {
        header.push("Output".into());
    }
    if show_cw {
        header.push("Cache Write".into());
    }
    if show_cr {
        header.push("Cache Read".into());
    }
    header.push("Total Tokens".into());
    header.push("Cost".into());
    style_header(&mut header, color);

    let first_numeric_col = 2 + usize::from(show_api) + usize::from(show_client);

    let mut builder = Builder::default();
    builder.push_record(header);

    for summary in &report.summaries {
        let total = summary.total_input
            + summary.total_output
            + summary.total_cache_creation()
            + summary.total_cache_read()
            + summary.total_thinking;

        let mut row: Vec<String> = vec![summary.label.clone(), String::new()];
        if show_api {
            row.push(String::new());
        }
        if show_client {
            row.push(String::new());
        }
        if show_in {
            row.push(format_tokens_styled(summary.total_input, color));
        }
        if show_out {
            row.push(format_tokens_styled(summary.total_output, color));
        }
        if show_cw {
            row.push(format_tokens_styled(summary.total_cache_creation(), color));
        }
        if show_cr {
            row.push(format_tokens_styled(summary.total_cache_read(), color));
        }
        row.push(format_tokens_styled(total, color));
        row.push(format_cost_styled(summary.total_cost, color));
        bold_row(&mut row, color);
        builder.push_record(row);

        // Build disambiguation suffixes for model names when columns are hidden
        let shortened: Vec<String> = summary
            .models
            .iter()
            .map(|m| display::display_model(&m.model))
            .collect();

        // Detect duplicates that need disambiguation
        let needs_suffix: Vec<Option<String>> = if show_api && show_client {
            // Both columns visible — no suffix needed
            vec![None; shortened.len()]
        } else {
            build_disambiguation_suffixes(&summary.models, &shortened, show_api, show_client)
        };

        for (i, model) in summary.models.iter().enumerate() {
            let model_total = model.total_tokens();

            let label = match &needs_suffix[i] {
                Some(suffix) => format!("  {} ({})", shortened[i], suffix),
                None => format!("  {}", shortened[i]),
            };
            let mut row: Vec<String> = vec![String::new(), label];
            if show_api {
                row.push(display::infer_api_provider(model.effective_raw_model()).to_string());
            }
            if show_client {
                row.push(display::display_client(&model.provider).into_owned());
            }
            if show_in {
                row.push(format_tokens_styled(model.input_tokens, color));
            }
            if show_out {
                row.push(format_tokens_styled(model.output_tokens, color));
            }
            if show_cw {
                row.push(format_tokens_styled(model.cache_creation_tokens, color));
            }
            if show_cr {
                row.push(format_tokens_styled(model.cache_read_tokens, color));
            }
            row.push(format_tokens_styled(model_total, color));
            row.push(format_cost_styled(model.cost_usd, color));
            builder.push_record(row);
        }
    }

    let (gi, go, gcw, gcr, gt) = grand_totals(report);
    let mut row: Vec<String> = vec!["TOTAL".into(), String::new()];
    if show_api {
        row.push(String::new());
    }
    if show_client {
        row.push(String::new());
    }
    if show_in {
        row.push(format_tokens(gi));
    }
    if show_out {
        row.push(format_tokens(go));
    }
    if show_cw {
        row.push(format_tokens(gcw));
    }
    if show_cr {
        row.push(format_tokens(gcr));
    }
    row.push(format_tokens(gt));
    row.push(format_cost(report.total_cost));
    bold_row(&mut row, color);
    builder.push_record(row);

    builder
        .build()
        .with(Style::rounded())
        .with(Modify::new(Columns::new(first_numeric_col..)).with(Alignment::right()))
        .to_string()
}

/// Build disambiguation suffixes for model sub-rows when Client/API Provider
/// columns are hidden and duplicate shortened model names exist.
fn build_disambiguation_suffixes(
    models: &[crate::types::ModelUsage],
    shortened: &[String],
    show_api: bool,
    show_client: bool,
) -> Vec<Option<String>> {
    use std::collections::HashMap;

    // Count occurrences of each shortened name
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for s in shortened {
        *counts.entry(s.as_str()).or_default() += 1;
    }

    shortened
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if counts[s.as_str()] <= 1 {
                return None; // No duplicate, no suffix needed
            }

            let mut parts: Vec<String> = Vec::new();
            if !show_api {
                let api = display::infer_api_provider(models[i].effective_raw_model());
                if !api.is_empty() {
                    parts.push(api.to_string());
                }
            }
            if !show_client {
                parts.push(display::display_client(&models[i].provider).into_owned());
            }

            if parts.is_empty() {
                None
            } else {
                Some(parts.join(", "))
            }
        })
        .collect()
}

fn print_compact_table(report: &Report, col_cfg: &crate::config::ColumnConfig) {
    let color = use_color();
    let width = terminal_width();

    let column_sets = [
        (true, true, true, true),
        (true, true, false, true),
        (true, true, false, false),
        (false, false, false, false),
    ];

    for &(show_in, show_out, show_cw, show_cr) in &column_sets {
        let masked = (
            show_in && col_cfg.input,
            show_out && col_cfg.output,
            show_cw && col_cfg.cache_write,
            show_cr && col_cfg.cache_read,
        );
        let table = render_compact(report, color, masked.0, masked.1, masked.2, masked.3);
        let first_line = table.lines().next().unwrap_or("");
        if display_width(first_line) <= width || (!masked.0 && !masked.1) {
            println!("{table}");
            return;
        }
    }
}

#[allow(clippy::fn_params_excessive_bools, clippy::similar_names)]
fn render_compact(
    report: &Report,
    color: bool,
    show_in: bool,
    show_out: bool,
    show_cw: bool,
    show_cr: bool,
) -> String {
    let mut header: Vec<String> = vec!["Date".into()];
    if show_in {
        header.push("Input".into());
    }
    if show_out {
        header.push("Output".into());
    }
    if show_cw {
        header.push("Cache Write".into());
    }
    if show_cr {
        header.push("Cache Read".into());
    }
    header.push("Total Tokens".into());
    header.push("Cost".into());
    style_header(&mut header, color);

    let first_numeric_col = 1;

    let mut builder = Builder::default();
    builder.push_record(header);

    for summary in &report.summaries {
        let total = summary.total_input
            + summary.total_output
            + summary.total_cache
            + summary.total_thinking;

        let mut row: Vec<String> = vec![summary.label.clone()];
        if show_in {
            row.push(format_tokens_styled(summary.total_input, color));
        }
        if show_out {
            row.push(format_tokens_styled(summary.total_output, color));
        }
        if show_cw {
            row.push(format_tokens_styled(summary.total_cache_creation(), color));
        }
        if show_cr {
            row.push(format_tokens_styled(summary.total_cache_read(), color));
        }
        row.push(format_tokens_styled(total, color));
        row.push(format_cost_styled(summary.total_cost, color));
        builder.push_record(row);
    }

    let (gi, go, gcw, gcr, gt) = grand_totals(report);
    let mut row: Vec<String> = vec!["TOTAL".into()];
    if show_in {
        row.push(format_tokens(gi));
    }
    if show_out {
        row.push(format_tokens(go));
    }
    if show_cw {
        row.push(format_tokens(gcw));
    }
    if show_cr {
        row.push(format_tokens(gcr));
    }
    row.push(format_tokens(gt));
    row.push(format_cost(report.total_cost));
    bold_row(&mut row, color);
    builder.push_record(row);

    builder
        .build()
        .with(Style::rounded())
        .with(Modify::new(Columns::new(first_numeric_col..)).with(Alignment::right()))
        .to_string()
}

fn grand_totals(report: &Report) -> (u64, u64, u64, u64, u64) {
    let gi: u64 = report.summaries.iter().map(|s| s.total_input).sum();
    let go: u64 = report.summaries.iter().map(|s| s.total_output).sum();
    let gcw: u64 = report
        .summaries
        .iter()
        .map(super::types::DailySummary::total_cache_creation)
        .sum();
    let gcr: u64 = report
        .summaries
        .iter()
        .map(super::types::DailySummary::total_cache_read)
        .sum();
    let gth: u64 = report.summaries.iter().map(|s| s.total_thinking).sum();
    (gi, go, gcw, gcr, gi + go + gcw + gcr + gth)
}

pub fn print_statusline(
    total_cost: f64,
    total_tokens: u64,
    provider_count: usize,
    period_label: &str,
) {
    let provider_str = if provider_count == 1 {
        "1 provider".to_string()
    } else {
        format!("{provider_count} providers")
    };

    println!(
        "{} | {} | {} | {}",
        format_cost(total_cost),
        format_tokens_short(total_tokens),
        provider_str,
        period_label
    );
}

pub fn print_budget(
    daily: Option<(f64, f64)>,
    weekly: Option<(f64, f64)>,
    monthly: Option<(f64, f64)>,
) {
    let lines = [("Daily", daily), ("Weekly", weekly), ("Monthly", monthly)];

    let mut any = false;
    for (label, budget) in &lines {
        if let Some((spent, limit)) = budget {
            any = true;
            let pct = if *limit > 0.0 {
                spent / limit * 100.0
            } else {
                0.0
            };
            let bar = progress_bar(pct, 10);
            let status = if pct > 100.0 {
                "OVER"
            } else if pct > 90.0 {
                "!!"
            } else if pct > 60.0 {
                "!"
            } else {
                "ok"
            };
            println!(
                "{:<8} ${:>8.2} / ${:<8.2} [{}] {:>5.1}%  {}",
                format!("{}:", label),
                spent,
                limit,
                bar,
                pct,
                status
            );
        }
    }

    if !any {
        println!("No budgets configured. Set [budget] in ~/.config/tokemon/config.toml");
    }
}

fn progress_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).min(width as f64) as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "#".repeat(filled), "-".repeat(empty))
}

#[must_use]
pub fn format_tokens_short(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

pub fn print_json(report: &Report) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[tokemon] Error serializing report: {e}"),
    }
}

pub fn print_discover(providers: &[(&str, &str, bool, String, usize)]) {
    let rows: Vec<DiscoverRow> = providers
        .iter()
        .map(
            |(name, display, available, data_dir, file_count)| DiscoverRow {
                provider: format!("{display} ({name})"),
                status: if *available {
                    "Found".to_string()
                } else {
                    "None".to_string()
                },
                data_dir: data_dir.clone(),
                files: format_tokens(*file_count as u64),
            },
        )
        .collect();

    let table = Table::new(&rows).with(Style::rounded()).to_string();
    println!("{table}");
}

fn format_tokens(n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

// ---------------------------------------------------------------------------
// CSV output
// ---------------------------------------------------------------------------

fn csv_quote(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn print_csv_compact(report: &Report) {
    println!("date,input,output,cache_write,cache_read,thinking,total_tokens,cost");
    for s in &report.summaries {
        let total = s.total_input + s.total_output + s.total_cache + s.total_thinking;
        println!(
            "{},{},{},{},{},{},{},{:.2}",
            csv_quote(&s.label),
            s.total_input,
            s.total_output,
            s.total_cache_creation(),
            s.total_cache_read(),
            s.total_thinking,
            total,
            s.total_cost
        );
    }
}

pub fn print_csv_breakdown(report: &Report) {
    println!("date,model,api_provider,client,input,output,cache_write,cache_read,thinking,total_tokens,cost");
    for s in &report.summaries {
        for m in &s.models {
            let model_total = m.total_tokens();
            println!(
                "{},{},{},{},{},{},{},{},{},{},{:.2}",
                csv_quote(&s.label),
                csv_quote(&display::display_model(&m.model)),
                csv_quote(display::infer_api_provider(m.effective_raw_model())),
                csv_quote(&display::display_client(&m.provider)),
                m.input_tokens,
                m.output_tokens,
                m.cache_creation_tokens,
                m.cache_read_tokens,
                m.thinking_tokens,
                model_total,
                m.cost_usd
            );
        }
    }
}

pub fn print_csv_sessions(report: &SessionReport) {
    println!("session_id,date,client,model,input,output,cache_write,cache_read,thinking,total_tokens,cost");
    for s in &report.sessions {
        let sid = if s.session_id.len() > 8 {
            &s.session_id[..8]
        } else {
            &s.session_id
        };
        println!(
            "{},{},{},{},{},{},{},{},{},{},{:.2}",
            csv_quote(sid),
            s.date.format("%Y-%m-%d"),
            csv_quote(&s.client),
            csv_quote(&s.dominant_model),
            s.input_tokens,
            s.output_tokens,
            s.cache_creation_tokens,
            s.cache_read_tokens,
            s.thinking_tokens,
            s.total_tokens,
            s.cost
        );
    }
}

pub fn print_sessions_table(report: &SessionReport) {
    let color = use_color();

    let mut header: Vec<String> = vec![
        "Session".into(),
        "Date".into(),
        "Client".into(),
        "Model".into(),
        "Total Tokens".into(),
        "Cost".into(),
    ];
    style_header(&mut header, color);

    let mut builder = Builder::default();
    builder.push_record(header);

    for s in &report.sessions {
        let sid = if s.session_id.len() > 8 {
            &s.session_id[..8]
        } else {
            &s.session_id
        };
        let row: Vec<String> = vec![
            sid.to_string(),
            s.date.format("%Y-%m-%d").to_string(),
            s.client.clone(),
            s.dominant_model.clone(),
            format_tokens_styled(s.total_tokens, color),
            format_cost_styled(s.cost, color),
        ];
        builder.push_record(row);
    }

    let count_label = format!("TOTAL ({})", report.sessions.len());
    let mut row: Vec<String> = vec![
        count_label,
        String::new(),
        String::new(),
        String::new(),
        format_tokens(report.total_tokens),
        format_cost(report.total_cost),
    ];
    bold_row(&mut row, color);
    builder.push_record(row);

    let table = builder
        .build()
        .with(Style::rounded())
        .with(Modify::new(Columns::new(4..)).with(Alignment::right()))
        .to_string();

    println!("{table}");
}

pub fn print_sessions_json(report: &SessionReport) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[tokemon] Error serializing sessions: {e}"),
    }
}

/// Format a USD cost value for display.
///
/// Rounds to 4 decimal places first (avoids float jitter in live TUI),
/// then selects precision based on magnitude:
/// - `$0.00` for zero
/// - `$0.0012` (4dp) for values under 1 cent
/// - `$123` (0dp) for values >= $100
/// - `$1.23` (2dp) for everything else
#[must_use]
pub fn format_cost(cost: f64) -> String {
    let rounded = (cost * 10_000.0).round() / 10_000.0;
    if rounded == 0.0 {
        "$0.00".to_string()
    } else if rounded < 0.01 {
        format!("${rounded:.4}")
    } else if rounded >= 100.0 {
        format!("${rounded:.0}")
    } else {
        format!("${rounded:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(123), "123");
        assert_eq!(format_tokens(1234), "1,234");
        assert_eq!(format_tokens(1234567), "1,234,567");
    }

    #[test]
    fn test_display_model() {
        use crate::display;
        assert_eq!(
            display::display_model("claude-opus-4-1-20250805"),
            "opus-4-1"
        );
        assert_eq!(
            display::display_model("claude-sonnet-4-20250514"),
            "sonnet-4"
        );
        assert_eq!(
            display::display_model("claude-opus-4-5-20251101"),
            "opus-4-5"
        );
        assert_eq!(display::display_model("gpt-5-codex"), "gpt-5-codex");
        assert_eq!(
            display::display_model("gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
        assert_eq!(
            display::display_model("vertexai.gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
        assert_eq!(display::display_model("openai/gpt-4o"), "gpt-4o");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(1.50), "$1.50");
        assert_eq!(format_cost(0.005), "$0.0050");
    }

    #[test]
    fn test_use_color_does_not_panic() {
        // Just ensure it runs without panicking in test context
        let _ = use_color();
    }

    #[test]
    fn test_format_cost_styled_no_color() {
        // Without color, should return plain string
        assert_eq!(format_cost_styled(0.0, false), "$0.00");
        assert_eq!(format_cost_styled(1.50, false), "$1.50");
    }

    #[test]
    fn test_format_tokens_styled_no_color() {
        assert_eq!(format_tokens_styled(0, false), "0");
        assert_eq!(format_tokens_styled(1234, false), "1,234");
    }

    #[test]
    fn test_csv_quote_plain() {
        assert_eq!(csv_quote("hello"), "hello");
        assert_eq!(csv_quote("2026-02-20"), "2026-02-20");
    }

    #[test]
    fn test_csv_quote_with_comma() {
        assert_eq!(csv_quote("hello, world"), "\"hello, world\"");
    }

    #[test]
    fn test_csv_quote_with_quotes() {
        assert_eq!(csv_quote("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_csv_quote_with_newline() {
        assert_eq!(csv_quote("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn test_csv_quote_with_carriage_return() {
        assert_eq!(csv_quote("line1\r\nline2"), "\"line1\r\nline2\"");
        assert_eq!(csv_quote("text\r"), "\"text\r\"");
    }

    #[test]
    fn test_csv_quote_empty() {
        assert_eq!(csv_quote(""), "");
    }
}
