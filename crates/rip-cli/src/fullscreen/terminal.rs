//! RAII terminal lifecycle.
//!
//! `TerminalGuard` disables raw mode, leaves the alt screen, restores
//! mouse capture state, and re-shows the cursor when the TUI exits —
//! on normal shutdown *and* on panic (`Drop`). This is the single
//! defence against a crashed TUI leaving the user's terminal in a
//! broken state.

use std::io;

use crossterm::event::DisableMouseCapture;
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub(super) struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    pub(super) fn active() -> Self {
        Self { active: true }
    }

    pub(super) fn deactivate(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> anyhow::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;

        disable_raw_mode()?;
        terminal.backend_mut().execute(DisableMouseCapture)?;
        terminal.backend_mut().execute(LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, DisableMouseCapture);
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
}
