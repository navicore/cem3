//! TUI REPL for the Seq programming language
//!
//! Provides a split-pane terminal interface with:
//! - REPL input with syntax highlighting and Vi mode editing
//! - Real-time IR visualization (stack effects, typed AST, LLVM snippets)
//! - ASCII art stack effect diagrams

pub mod app;
pub mod engine;
pub mod ir;
pub mod ui;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, stdout};

/// Run the TUI REPL with an optional file
pub fn run(file: Option<&std::path::Path>) -> Result<(), String> {
    // Setup terminal
    enable_raw_mode().map_err(|e| format!("Failed to enable raw mode: {}", e))?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| format!("Failed to enter alternate screen: {}", e))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("Failed to create terminal: {}", e))?;

    // Create app with file if provided, otherwise use temp file
    let app_state = if let Some(path) = file {
        app::App::with_file(path.to_path_buf())
    } else {
        app::App::new()
    };

    // Run the app
    let result = run_app(&mut terminal, app_state);

    // Restore terminal
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result.map_err(|e| format!("Application error: {}", e))
}

/// Internal run loop (specialized for CrosstermBackend)
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    mut app: app::App,
) -> io::Result<()> {
    use crossterm::event::{self, Event};
    use std::time::Duration;

    loop {
        terminal.draw(|frame| app.render(frame))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
