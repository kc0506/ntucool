pub mod browser;
pub mod fuzzy_select;

use std::io;

use crossterm::{
    cursor,
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, style::Color, Terminal};

/// Set up a ratatui Terminal with alternate screen.
pub(crate) fn setup_terminal() -> io::Result<(Terminal<CrosstermBackend<io::Stdout>>, TerminalGuard)>
{
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok((terminal, TerminalGuard))
}

/// RAII guard that restores the terminal on drop (including panics).
pub(crate) struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
    }
}

// ── Color palette ────────────────────────────────────────────────────────────
// NTU COOL: orange + blue. Blue as primary, orange as accent.

pub(crate) mod theme {
    use super::Color;

    /// Primary blue — borders, titles, section headers
    pub const BLUE: Color = Color::Rgb(90, 145, 210);
    /// Dimmer blue — subtle borders, secondary text
    pub const BLUE_DIM: Color = Color::Rgb(55, 90, 140);
    /// Accent orange — selection highlight, active focus, interactive elements
    pub const ORANGE: Color = Color::Rgb(230, 145, 55);
    /// Foreground text
    pub const FG: Color = Color::Rgb(220, 220, 225);
    /// Muted / dim text
    pub const MUTED: Color = Color::Rgb(110, 115, 125);
    /// Success green
    pub const GREEN: Color = Color::Rgb(80, 200, 120);
    /// Error red
    pub const RED: Color = Color::Rgb(220, 85, 85);
    /// Dark background for highlighted rows
    pub const HIGHLIGHT_BG: Color = Color::Rgb(35, 55, 85);
}
