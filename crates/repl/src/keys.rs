//! Key conversion from crossterm to vim-line.

use crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyModifiers};
use vim_line::{Key, KeyCode};

/// Convert a crossterm KeyEvent to a vim-line Key.
pub fn convert_key(event: KeyEvent) -> Key {
    let code = match event.code {
        CtKeyCode::Char(c) => KeyCode::Char(c),
        CtKeyCode::Esc => KeyCode::Escape,
        CtKeyCode::Backspace => KeyCode::Backspace,
        CtKeyCode::Delete => KeyCode::Delete,
        CtKeyCode::Left => KeyCode::Left,
        CtKeyCode::Right => KeyCode::Right,
        CtKeyCode::Up => KeyCode::Up,
        CtKeyCode::Down => KeyCode::Down,
        CtKeyCode::Home => KeyCode::Home,
        CtKeyCode::End => KeyCode::End,
        CtKeyCode::Tab => KeyCode::Tab,
        CtKeyCode::Enter => KeyCode::Enter,
        // Map unsupported keys to a placeholder
        _ => return Key::code(KeyCode::Escape), // Ignored
    };

    Key {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
    }
}
