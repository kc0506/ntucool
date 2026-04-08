#![allow(dead_code)]

mod app;
mod data;
mod event;
mod theme;
mod ui;

use std::io;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use app::App;
use event::EventHandler;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and event handler
    let mut app = App::new();
    let mut events = EventHandler::new(250); // 250ms tick rate

    // Main loop
    let res = run_app(&mut terminal, &mut app, &mut events).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    events: &mut EventHandler,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        match events.next().await? {
            event::Event::Tick => {
                app.on_tick().await;
            }
            event::Event::Key(key_event) => {
                app.on_key(key_event).await;
            }
            event::Event::Mouse(_) => {}
            event::Event::Resize(_, _) => {}
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
