use ratatui::style::{Color, Modifier, Style};

// ── Base palette ──────────────────────────────────────────────────────────

/// Deep background — the terminal canvas.
pub const BG: Color = Color::Rgb(15, 17, 22);

/// Slightly lighter surface for panels / cards.
pub const SURFACE: Color = Color::Rgb(22, 25, 33);

/// Borders, separators.
pub const BORDER: Color = Color::Rgb(48, 54, 68);

/// Subtle text (labels, inactive items).
pub const DIM: Color = Color::Rgb(88, 96, 112);

/// Normal text.
pub const FG: Color = Color::Rgb(200, 205, 215);

/// Bright / emphasized text.
pub const FG_BRIGHT: Color = Color::Rgb(235, 238, 245);

// ── Accent colours ────────────────────────────────────────────────────────

/// Primary accent — brand / active tab / highlights.
pub const ACCENT: Color = Color::Rgb(99, 140, 255);

/// Secondary accent — less prominent highlights.
pub const ACCENT_DIM: Color = Color::Rgb(65, 95, 180);

/// Success / positive values.
pub const GREEN: Color = Color::Rgb(80, 200, 120);

/// Warning / moderate values.
pub const YELLOW: Color = Color::Rgb(230, 190, 60);

/// Error / high values.
pub const RED: Color = Color::Rgb(235, 85, 85);

/// Cyan for headers and labels.
pub const CYAN: Color = Color::Rgb(85, 205, 220);

/// Highlight colour for updated cell text — bright green that fades to normal.
pub const HIGHLIGHT_GREEN: Color = Color::Rgb(80, 220, 110);

// ── Composite styles ──────────────────────────────────────────────────────

/// Default text style.
#[must_use]
pub fn text() -> Style {
    Style::default().fg(FG).bg(BG)
}

/// Dimmed / secondary text.
#[must_use]
pub fn text_dim() -> Style {
    Style::default().fg(DIM).bg(BG)
}

/// Bold bright text.
#[must_use]
pub fn text_bold() -> Style {
    Style::default()
        .fg(FG_BRIGHT)
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

/// Header / column label style.
#[must_use]
pub fn header() -> Style {
    Style::default()
        .fg(CYAN)
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

/// Active tab indicator (Phase 3).
#[must_use]
#[allow(dead_code)]
pub fn tab_active() -> Style {
    Style::default()
        .fg(BG)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

/// Inactive tab (Phase 3).
#[must_use]
#[allow(dead_code)]
pub fn tab_inactive() -> Style {
    Style::default().fg(DIM).bg(SURFACE)
}

/// Border style.
#[must_use]
pub fn border() -> Style {
    Style::default().fg(BORDER).bg(BG)
}

/// Cost foreground color based on value.
#[must_use]
pub fn cost_color(cost: f64) -> Color {
    if cost == 0.0 {
        DIM
    } else if cost < 1.0 {
        GREEN
    } else if cost < 10.0 {
        YELLOW
    } else {
        RED
    }
}

/// Token foreground color based on value (dim for zeros).
#[must_use]
pub fn tokens_color(n: u64) -> Color {
    if n == 0 {
        DIM
    } else {
        FG
    }
}

/// Surface panel style (for cards).
#[must_use]
pub fn card() -> Style {
    Style::default().fg(FG).bg(SURFACE)
}

/// Card title / label.
#[must_use]
pub fn card_label() -> Style {
    Style::default()
        .fg(DIM)
        .bg(SURFACE)
        .add_modifier(Modifier::BOLD)
}

/// Card value (large number).
#[must_use]
pub fn card_value() -> Style {
    Style::default()
        .fg(FG_BRIGHT)
        .bg(SURFACE)
        .add_modifier(Modifier::BOLD)
}

/// Card secondary value (tokens).
#[must_use]
pub fn card_secondary() -> Style {
    Style::default().fg(DIM).bg(SURFACE)
}

/// Status bar background.
#[must_use]
pub fn status_bar() -> Style {
    Style::default().fg(DIM).bg(SURFACE)
}

/// Status bar keybinding highlight.
#[must_use]
pub fn status_key() -> Style {
    Style::default()
        .fg(ACCENT)
        .bg(SURFACE)
        .add_modifier(Modifier::BOLD)
}

/// Total row (bold).
#[must_use]
pub fn total_row() -> Style {
    Style::default()
        .fg(FG_BRIGHT)
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

/// Highlighted cell style for updated token/cost values.
///
/// `intensity` ranges from 1.0 (just updated) to 0.0 (fully faded).
/// At full intensity: bright green text + bold.
/// As it fades: text colour interpolates back to `normal_fg`.
#[must_use]
pub fn highlight_cell(intensity: f64, normal_fg: Color) -> Style {
    if intensity <= 0.0 {
        return Style::default().fg(normal_fg).bg(BG);
    }

    let fg = lerp_color(normal_fg, HIGHLIGHT_GREEN, intensity);
    let mut style = Style::default().fg(fg).bg(BG);
    // Bold for the first ~60% of the animation
    if intensity > 0.4 {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

/// Linearly interpolate between two RGB colours.
fn lerp_color(from: Color, to: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let r = (f64::from(r1) + (f64::from(r2) - f64::from(r1)) * t) as u8;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let g = (f64::from(g1) + (f64::from(g2) - f64::from(g1)) * t) as u8;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let b = (f64::from(b1) + (f64::from(b2) - f64::from(b1)) * t) as u8;
            Color::Rgb(r, g, b)
        }
        // If not both RGB, just return target at high intensity, source otherwise
        _ => {
            if t > 0.5 {
                to
            } else {
                from
            }
        }
    }
}
