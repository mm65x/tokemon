mod app;
pub(crate) mod diff;
mod event;
pub(crate) mod settings_state;
mod sparkline_data;
mod terminal;
mod theme;
mod views;
mod watcher;
mod widgets;

use std::time::Duration;

use ratatui::layout::Rect;

use crate::config::Config;
use app::{App, Scope};
use event::{Event, EventHandler};

/// Default data poll interval (seconds).
const DEFAULT_TICK_SECS: u64 = 2;

/// Target frame rate for rendering.
const RENDER_FPS: u64 = 30;

/// Run the TUI dashboard.
///
/// This is the entry point called from `main.rs` when the user runs
/// `tokemon top`. It sets up the terminal, event loop, and runs until
/// the user quits.
///
/// # Errors
///
/// Returns an error if terminal initialisation fails.
pub fn run(
    config: &Config,
    initial_view: &str,
    tick_interval: u64,
    offline: bool,
) -> anyhow::Result<()> {
    let scope = match initial_view {
        "week" | "w" => Scope::Week,
        "month" | "m" => Scope::Month,
        _ => Scope::Today,
    };

    let tick_secs = if tick_interval == 0 {
        DEFAULT_TICK_SECS
    } else {
        tick_interval
    };

    // Build a single-threaded tokio runtime for the async event loop.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async { run_async(config, scope, tick_secs, offline).await })
}

async fn run_async(
    config: &Config,
    scope: Scope,
    tick_secs: u64,
    offline: bool,
) -> anyhow::Result<()> {
    let mut terminal = terminal::init()?;
    let mut terminal_area = Rect::from(terminal.size()?);
    let mut app = App::new(config, scope, offline);

    let mut events = EventHandler::new(
        Duration::from_secs(tick_secs),
        Duration::from_millis(1000 / RENDER_FPS),
    );
    events.start();

    // Start the file watcher in the background.
    // It will send Event::DataChanged when source files are modified.
    let event_tx = events.sender();
    watcher::start(event_tx);

    // Main loop
    let mut was_animating = false;
    loop {
        let Some(mut event) = events.next().await else {
            break;
        };

        loop {
            match &event {
                Event::Render => {} // render ticks don't mark state dirty
                other => {
                    if let Event::Resize(width, height) = other {
                        terminal_area = Rect::new(0, 0, *width, *height);
                    }
                    app.handle_event(other, terminal_area);
                }
            }

            if app.should_quit {
                break;
            }

            match events.try_next() {
                Ok(next_event) => {
                    event = next_event;
                }
                Err(_) => {
                    break;
                }
            }
        }

        if app.should_quit {
            break;
        }

        let is_animating = app.has_active_highlights();
        if was_animating && !is_animating {
            app.dirty = true;
        }
        was_animating = is_animating;

        // Only redraw when state changed or highlight animations are fading
        if app.dirty || is_animating {
            terminal.draw(|frame| {
                views::dashboard::render(frame, &app);
            })?;
            app.dirty = false;
        }
    }

    Ok(terminal.finish()?)
}
