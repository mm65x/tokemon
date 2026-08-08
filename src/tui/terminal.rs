use std::io::{self, Stdout};
use std::panic;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Owns an initialised terminal session and restores it on every exit path.
pub struct TerminalSession {
    terminal: Tui,
    active: bool,
}

impl TerminalSession {
    /// Restore the terminal and consume the session.
    pub fn finish(mut self) -> io::Result<()> {
        restore()?;
        self.active = false;
        Ok(())
    }
}

impl std::ops::Deref for TerminalSession {
    type Target = Tui;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl std::ops::DerefMut for TerminalSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.active {
            let _ = restore();
        }
    }
}

/// Initialise the terminal: raw mode, alternate screen, mouse capture.
/// Returns a session that restores the terminal when dropped.
///
/// # Errors
///
/// Returns an error if terminal initialisation fails.
pub fn init() -> io::Result<TerminalSession> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
        let _ = restore();
        return Err(error);
    }
    let backend = CrosstermBackend::new(stdout);
    let terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = restore();
            return Err(error);
        }
    };

    // Install a panic hook that restores the terminal before printing the
    // panic message — otherwise the user is left with a broken terminal.
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore();
        original_hook(info);
    }));

    Ok(TerminalSession {
        terminal,
        active: true,
    })
}

/// Restore the terminal to its original state.
///
/// # Errors
///
/// Returns an error if terminal restoration fails.
pub fn restore() -> io::Result<()> {
    let screen_result = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    let raw_result = disable_raw_mode();
    screen_result.and(raw_result)
}
