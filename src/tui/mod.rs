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
pub fn run(config: &Config, initial_view: &str, tick_interval: u64) -> anyhow::Result<()> {
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

    runtime.block_on(async { run_async(config, scope, tick_secs).await })
}

async fn run_async(config: &Config, scope: Scope, tick_secs: u64) -> anyhow::Result<()> {
    let mut terminal = terminal::init()?;
    let mut app = App::new(config, scope);

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
    loop {
        let Some(event) = events.next().await else {
            break;
        };

        match &event {
            Event::Render => {} // render ticks don't mark state dirty
            other => {
                app.handle_event(other);
            }
        }

        if app.should_quit {
            break;
        }

        // Only redraw when state changed or highlight animations are fading
        if app.dirty || app.has_active_highlights() {
            terminal.draw(|frame| {
                views::dashboard::render(frame, &app);
            })?;
            app.dirty = false;
        }
    }

    terminal::restore()?;
    Ok(())
}
