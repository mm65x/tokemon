use tabled::builder::Builder;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Modify, Style};
use tabled::{Table, Tabled};

use crate::display;
use crate::types::{Report, SessionReport};

use super::helpers::{
    bold_row, display_width, format_cost, format_cost_styled, format_tokens, format_tokens_short,
    format_tokens_styled, style_header, terminal_width, use_color,
};

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
        let table = render_compact(report, color, masked);
        let first_line = table.lines().next().unwrap_or("");
        if display_width(first_line) <= width || (!masked.show_in && !masked.show_out) {
            println!("{table}");
            return;
        }
    }
}

#[allow(clippy::similar_names)]
fn render_compact(report: &Report, color: bool, cols: BreakdownCols) -> String {
    let BreakdownCols {
        show_in,
        show_out,
        show_cw,
        show_cr,
        ..
    } = cols;
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
            + summary.total_cache_creation()
            + summary.total_cache_read()
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
        .map(crate::types::PeriodSummary::total_cache_creation)
        .sum();
    let gcr: u64 = report
        .summaries
        .iter()
        .map(crate::types::PeriodSummary::total_cache_read)
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

pub fn print_budget(status: &crate::pacemaker::BudgetStatus) {
    let lines = [
        ("Daily", status.daily),
        ("Weekly", status.weekly),
        ("Monthly", status.monthly),
    ];

    let mut any = false;
    for (label, budget) in &lines {
        if let Some(bp) = budget {
            any = true;
            let (spent, limit) = (bp.spent, bp.limit);
            let pct = if limit > 0.0 {
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

#[cfg(test)]
mod tests {
    use crate::display;

    #[test]
    fn test_display_model() {
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
}
