/// Format a cost value with color coding.
pub fn format_cost_styled(cost: f64, color: bool) -> String {
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
pub fn format_tokens_styled(n: u64, color: bool) -> String {
    let s = format_tokens(n);
    if color && n == 0 {
        dim(&s, true)
    } else {
        s
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

pub fn format_tokens(n: u64) -> String {
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
// Color helpers
// ---------------------------------------------------------------------------

use std::io::IsTerminal;

/// Whether to emit ANSI color codes. Respects NO_COLOR and non-TTY pipes.
#[must_use]
pub fn use_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn ansi(code: &str, s: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str, c: bool) -> String {
    ansi("1", s, c)
}
pub fn dim(s: &str, c: bool) -> String {
    ansi("2", s, c)
}
pub fn cyan_bold(s: &str, c: bool) -> String {
    ansi("1;36", s, c)
}
pub fn green(s: &str, c: bool) -> String {
    ansi("32", s, c)
}
pub fn yellow(s: &str, c: bool) -> String {
    ansi("33", s, c)
}
pub fn red(s: &str, c: bool) -> String {
    ansi("31", s, c)
}

/// Apply bold to every element in a row.
pub fn bold_row(row: &mut [String], color: bool) {
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
pub fn style_header(header: &mut [String], color: bool) {
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
pub fn terminal_width() -> usize {
    terminal_size::terminal_size().map_or(120, |(w, _)| w.0 as usize)
}

/// Visible width of a string, ignoring ANSI escape codes.
#[must_use]
pub fn display_width(s: &str) -> usize {
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
}
