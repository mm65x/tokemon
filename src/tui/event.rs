use std::time::Duration;

use crossterm::event::{
    Event as CrosstermEvent, EventStream, KeyEventKind, MouseButton, MouseEventKind,
};
use futures_lite::StreamExt;
use tokio::sync::mpsc;

/// Application-level events.
#[derive(Debug, Clone)]
pub enum Event {
    /// A key was pressed.
    Key(crossterm::event::KeyEvent),
    /// A mouse button was pressed or the wheel was scrolled.
    Mouse(crossterm::event::MouseEvent),
    /// Terminal was resized (values used by ratatui's `frame.area()` implicitly).
    #[allow(dead_code)]
    Resize(u16, u16),
    /// Tick — time to poll for data updates.
    Tick,
    /// Render — time to redraw the UI.
    Render,
    /// The file watcher detected changes and updated the cache.
    DataChanged,
    /// A warning from the background watcher or data loading.
    /// Displayed briefly in the status bar instead of printing to stderr.
    Warning(String),
}

/// Drives the event loop, forwarding crossterm events and emitting periodic
/// tick / render events through an `mpsc` channel.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    tx: mpsc::UnboundedSender<Event>,
    tick_rate: Duration,
    render_rate: Duration,
}

impl EventHandler {
    /// Create a new event handler.
    ///
    /// * `tick_rate` — how often to emit `Event::Tick` (data poll interval).
    /// * `render_rate` — how often to emit `Event::Render` (frame rate).
    #[must_use]
    pub fn new(tick_rate: Duration, render_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            rx,
            tx,
            tick_rate,
            render_rate,
        }
    }

    /// Get a clone of the sender for external use (e.g. file watcher, Phase 2).
    #[must_use]
    #[allow(dead_code)]
    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.tx.clone()
    }

    /// Start the background event loop. This spawns a tokio task that
    /// reads crossterm events and emits tick/render events on intervals.
    pub fn start(&self) {
        let tx = self.tx.clone();
        let tick_rate = self.tick_rate;
        let render_rate = self.render_rate;

        tokio::spawn(async move {
            let mut crossterm_events = EventStream::new();
            let mut tick_interval = tokio::time::interval(tick_rate);
            let mut render_interval = tokio::time::interval(render_rate);

            tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                let event = tokio::select! {
                    // Crossterm terminal events (key presses, resize, etc.)
                    maybe_event = crossterm_events.next() => {
                        match maybe_event {
                            Some(Ok(evt)) => map_crossterm_event(&evt),
                            // Stream ended or error — stop the loop
                            Some(Err(_)) | None => break,
                        }
                    }
                    // Periodic tick for data refresh
                    _ = tick_interval.tick() => {
                        Some(Event::Tick)
                    }
                    // Periodic render
                    _ = render_interval.tick() => {
                        Some(Event::Render)
                    }
                };

                if let Some(e) = event {
                    if tx.send(e).is_err() {
                        break;
                    }
                }
            }
        });
    }

    /// Receive the next event. Returns `None` if the channel is closed.
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// Try to receive the next event without blocking.
    #[allow(dead_code)]
    pub fn try_next(&mut self) -> Result<Event, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}

fn map_crossterm_event(event: &CrosstermEvent) -> Option<Event> {
    match event {
        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => Some(Event::Key(*key)),
        CrosstermEvent::Mouse(mouse)
            if matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
            ) =>
        {
            Some(Event::Mouse(*mouse))
        }
        CrosstermEvent::Resize(width, height) => Some(Event::Resize(*width, *height)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    };

    use super::{map_crossterm_event, CrosstermEvent, Event};

    #[test]
    fn routes_mouse_events() {
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };

        let mapped = map_crossterm_event(&CrosstermEvent::Mouse(mouse));

        assert!(matches!(mapped, Some(Event::Mouse(event)) if event == mouse));
    }

    #[test]
    fn ignores_key_release_events() {
        let key = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };

        assert!(map_crossterm_event(&CrosstermEvent::Key(key)).is_none());
    }

    #[test]
    fn ignores_mouse_motion() {
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 12,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };

        assert!(map_crossterm_event(&CrosstermEvent::Mouse(mouse)).is_none());
    }
}
