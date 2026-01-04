//! seqr - TUI REPL for Seq
//!
//! A split-pane terminal REPL with real-time IR visualization.
//! Stack persists across lines - build up values incrementally.
//!
//! Usage:
//!   seqr                    # Start with temp file
//!   seqr myprogram.seq      # Start with existing file
//!
//! Features:
//!   - Split-pane interface (REPL left, IR right)
//!   - Vi-style editing with syntax highlighting
//!   - Real-time IR visualization (stack effects, typed AST, LLVM IR)
//!   - Tab for LSP completions, h/l to cycle IR views
//!
//! Commands:
//!   :quit, :q               # Exit
//!   :pop                    # Remove last expression (undo)
//!   :clear                  # Clear the session (reset stack)
//!   :show                   # Show current file contents
//!   :edit, :e               # Open file in $EDITOR
//!   :include <mod>          # Include a module (e.g., std:math)
//!   :help                   # Show help

mod app;
mod engine;
mod ir;
mod keys;
mod lsp_client;
mod ui;

use clap::Parser as ClapParser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, stdout};
use std::panic;
use std::path::PathBuf;

#[derive(ClapParser)]
#[command(name = "seqr")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "TUI REPL for Seq with IR visualization", long_about = None)]
struct Args {
    /// Seq source file to use (creates temp file if not specified)
    file: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(args.file.as_deref()) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Run the TUI REPL with an optional file
fn run(file: Option<&std::path::Path>) -> Result<(), String> {
    // Install panic hook to restore terminal on panic
    // This ensures the terminal is always left in a usable state
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal state before printing panic message
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        // Call original panic handler
        original_hook(panic_info);
    }));

    // Setup terminal with mouse capture for scroll events
    enable_raw_mode().map_err(|e| format!("Failed to enable raw mode: {}", e))?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| format!("Failed to enter alternate screen: {}", e))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("Failed to create terminal: {}", e))?;

    // Create app with file if provided, otherwise use temp file
    let app_state = if let Some(path) = file {
        app::App::with_file(path.to_path_buf())?
    } else {
        app::App::new()?
    };

    // Run the app
    let result = run_app(&mut terminal, app_state);

    // Restore terminal
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    result.map_err(|e| format!("Application error: {}", e))
}

/// Internal run loop (specialized for CrosstermBackend)
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    mut app: app::App,
) -> io::Result<()> {
    use crossterm::event::{self, Event, MouseEventKind};
    use std::time::Duration;

    loop {
        terminal.draw(|frame| app.render(frame))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => app.scroll_ir(1),
                    MouseEventKind::ScrollUp => app.scroll_ir(-1),
                    _ => {}
                },
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }

        if app.should_edit {
            app.should_edit = false;
            open_in_editor(terminal, &app.session_path)?;
        }
    }

    // Save history before exiting
    app.save_history();

    Ok(())
}

/// Open the session file in $EDITOR
fn open_in_editor(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    path: &std::path::Path,
) -> io::Result<()> {
    use std::process::Command;

    // Leave TUI mode
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    // Use shlex for safe shell-aware parsing (handles quotes, escapes, etc.)
    // This prevents command injection and handles edge cases like:
    // - EDITOR="code --wait"
    // - EDITOR="'/path with spaces/editor'"
    let parts = match shlex::split(&editor) {
        Some(p) if !p.is_empty() => p,
        _ => vec!["vi".to_string()],
    };

    // Run editor
    let status = Command::new(&parts[0]).args(&parts[1..]).arg(path).status();

    if let Err(e) = status {
        eprintln!("Failed to open editor: {}", e);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // Return to TUI mode
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    Ok(())
}
