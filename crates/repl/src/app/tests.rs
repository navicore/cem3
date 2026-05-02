use super::*;

#[test]
fn test_app_creation() -> Result<(), String> {
    let app = App::new()?;
    assert_eq!(app.editor.status(), "NORMAL");
    assert!(!app.should_quit);
    Ok(())
}

#[test]
fn test_mode_switching() -> Result<(), String> {
    let mut app = App::new()?;

    // i enters insert mode
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.editor.status(), "INSERT");

    // Esc returns to normal
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.editor.status(), "NORMAL");
    Ok(())
}

#[test]
fn test_insert_mode_typing() -> Result<(), String> {
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));

    app.handle_key(KeyEvent::from(KeyCode::Char('h')));
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));

    assert_eq!(app.repl_state.input, "hi");
    Ok(())
}

#[test]
fn test_normal_mode_navigation() -> Result<(), String> {
    let mut app = App::new()?;

    // Type "hello" in insert mode
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    for c in "hello".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.handle_key(KeyEvent::from(KeyCode::Esc));

    // Now in normal mode at position 4 (esc moves left one from end)
    // Move to start with 0
    app.handle_key(KeyEvent::from(KeyCode::Char('0')));
    assert_eq!(app.repl_state.cursor, 0);

    // l moves right
    app.handle_key(KeyEvent::from(KeyCode::Char('l')));
    assert_eq!(app.repl_state.cursor, 1);

    // h moves left
    app.handle_key(KeyEvent::from(KeyCode::Char('h')));
    assert_eq!(app.repl_state.cursor, 0);

    // $ goes to end (ON last char, not past it - vim normal mode behavior)
    app.handle_key(KeyEvent::from(KeyCode::Char('$')));
    assert_eq!(app.repl_state.cursor, 4); // 'o' is at index 4
    Ok(())
}

#[test]
fn test_history_navigation() -> Result<(), String> {
    let mut app = App::new()?;

    // Add some history entries manually
    app.repl_state
        .add_entry(HistoryEntry::new("first").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("second").with_output("2"));

    // Up arrow goes up in history (to most recent)
    app.handle_key(KeyEvent::from(KeyCode::Up));
    assert_eq!(app.repl_state.input, "second");

    // Up again goes to older entry
    app.handle_key(KeyEvent::from(KeyCode::Up));
    assert_eq!(app.repl_state.input, "first");

    // Down goes back to newer entry
    app.handle_key(KeyEvent::from(KeyCode::Down));
    assert_eq!(app.repl_state.input, "second");
    Ok(())
}

#[test]
fn test_jk_history_navigation() -> Result<(), String> {
    let mut app = App::new()?;

    // Add some history entries
    app.repl_state
        .add_entry(HistoryEntry::new("first").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("second").with_output("2"));

    // In normal mode (default), k on single line goes to history
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "second");

    // k again goes to older entry
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "first");

    // j goes back to newer entry
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.repl_state.input, "second");

    // j again returns to empty input (current line)
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.repl_state.input, "");
    Ok(())
}

#[test]
fn test_jk_multiline_navigation() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history entries: oldest first, then multi-line, then newest
    app.repl_state
        .add_entry(HistoryEntry::new("oldest").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("line1\nline2\nline3").with_output("2"));

    // Navigate to the multi-line history entry
    app.handle_key(KeyEvent::from(KeyCode::Char('k'))); // Go to newest (multi-line)
    assert_eq!(app.repl_state.input, "line1\nline2\nline3");

    // Cursor should be at end of last line after history navigation
    // j on last line should go forward in history (to current empty input)
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.repl_state.input, "");

    // k goes back to multi-line
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "line1\nline2\nline3");

    // Now test navigation within the multi-line buffer
    // Go to start of last line with 0
    app.handle_key(KeyEvent::from(KeyCode::Char('0')));

    // k should move up within buffer (to line2), not to history
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "line1\nline2\nline3"); // Still same input

    // k again moves to line1 (now on first line)
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "line1\nline2\nline3"); // Still same input

    // k on first line goes to older history
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "oldest");

    Ok(())
}

#[test]
fn test_jk_with_unicode() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history with emoji
    app.repl_state
        .add_entry(HistoryEntry::new("hello 👋").with_output("1"));

    // k should navigate to history without panicking
    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "hello 👋");

    // j should navigate back
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.repl_state.input, "");

    Ok(())
}

#[test]
fn test_jk_empty_input() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history so we have somewhere to navigate
    app.repl_state
        .add_entry(HistoryEntry::new("history").with_output("1"));

    // With empty input, j/k should work without panic
    assert_eq!(app.repl_state.input, "");
    app.handle_key(KeyEvent::from(KeyCode::Char('k'))); // Should go to history
    assert_eq!(app.repl_state.input, "history");

    app.handle_key(KeyEvent::from(KeyCode::Char('j'))); // Should return to empty
    assert_eq!(app.repl_state.input, "");

    Ok(())
}

#[test]
fn test_jk_insert_mode_no_history() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history
    app.repl_state
        .add_entry(HistoryEntry::new("history").with_output("1"));

    // Enter insert mode
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.editor.status(), "INSERT");

    // j/k in insert mode should NOT navigate history - they should be passed
    // to vim-line which handles them as cursor movement (no-op on single line)
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    assert_eq!(app.repl_state.input, "j"); // j inserted as text

    app.handle_key(KeyEvent::from(KeyCode::Char('k')));
    assert_eq!(app.repl_state.input, "jk"); // k inserted as text

    Ok(())
}

#[test]
fn test_quit_command() -> Result<(), String> {
    let mut app = App::new()?;
    // Ctrl+Q quits the application
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert!(app.should_quit);
    Ok(())
}

#[test]
fn test_repl_command() -> Result<(), String> {
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.should_quit);
    Ok(())
}

#[test]
fn test_command_line_quit_from_normal() -> Result<(), String> {
    // Checkpoint 5: from Normal, `:` `q` Enter quits — without entering
    // Insert and without writing `:q` into the main input buffer.
    let mut app = App::new()?;
    assert_eq!(app.editor.status(), "NORMAL");

    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    assert_eq!(
        app.editor.status(),
        "COMMAND",
        "`:` from Normal must enter command-line mode"
    );
    assert_eq!(
        app.repl_state.input, "",
        "main input must stay empty while typing in the command line"
    );

    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    assert_eq!(app.editor.command_line_buffer(), Some("q"));
    assert_eq!(app.repl_state.input, "");

    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.should_quit, "Enter on `:q` must quit");
    Ok(())
}

#[test]
fn test_command_line_escape_cancels() -> Result<(), String> {
    // Esc from CommandLine returns to Normal with the main buffer
    // unchanged and no command dispatched.
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    for c in "stack".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    assert_eq!(app.editor.command_line_buffer(), Some("stack"));

    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.editor.status(), "NORMAL");
    assert_eq!(app.editor.command_line_buffer(), None);
    assert_eq!(app.repl_state.input, "");
    assert!(!app.should_quit);
    Ok(())
}

#[test]
fn test_cmdline_tab_cycles_matches() -> Result<(), String> {
    // From `:i` Tab should cycle through every command starting with "i":
    // `include `, `ir`, `ir ast`, `ir llvm`, `ir stack`, then back to the
    // original `i` prefix, then around again.
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.editor.command_line_buffer(), Some("i"));

    let expected = [
        "include ", "ir", "ir ast", "ir llvm", "ir stack", "i", // original prefix
        "include ",
    ];
    for want in expected {
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(
            app.editor.command_line_buffer(),
            Some(want),
            "Tab cycle expected {:?}",
            want
        );
    }
    Ok(())
}

#[test]
fn test_cmdline_shift_tab_cycles_reverse() -> Result<(), String> {
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));

    // BackTab on first invocation should jump straight to the LAST match.
    app.handle_key(KeyEvent::from(KeyCode::BackTab));
    assert_eq!(app.editor.command_line_buffer(), Some("ir stack"));
    // And step backwards from there.
    app.handle_key(KeyEvent::from(KeyCode::BackTab));
    assert_eq!(app.editor.command_line_buffer(), Some("ir llvm"));
    Ok(())
}

#[test]
fn test_cmdline_tab_resets_cycle_after_typing() -> Result<(), String> {
    // Tab cycle is keyed off the buffer at the moment cycling began;
    // typing any other key must rebuild it next time Tab is pressed.
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.editor.command_line_buffer(), Some("quit"));
    // Tab again would advance, but we're going to type instead.
    app.handle_key(KeyEvent::from(KeyCode::Backspace));
    // Back to the user's prefix `qui` (Backspace removed `t`).
    assert_eq!(app.editor.command_line_buffer(), Some("qui"));
    // Cycle should rebuild from `qui` — only one match now.
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.editor.command_line_buffer(), Some("quit"));
    Ok(())
}

#[test]
fn test_cmdline_tab_no_matches_is_noop() -> Result<(), String> {
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('z')));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.editor.command_line_buffer(), Some("z"));
    Ok(())
}

#[test]
fn test_cmdline_completion_then_submit_dispatches() -> Result<(), String> {
    // End-to-end: Tab-complete `:q` to `:quit`, hit Enter, app quits.
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.editor.command_line_buffer(), Some("quit"));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.should_quit);
    Ok(())
}

#[test]
fn test_cmdline_tab_in_insert_mode_unchanged() -> Result<(), String> {
    // Outside CommandLine, Tab still triggers main-buffer completion —
    // not command cycling.
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    app.handle_key(KeyEvent::from(KeyCode::Char('d')));
    app.handle_key(KeyEvent::from(KeyCode::Char('u')));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert!(
        app.completions.is_visible(),
        "Tab in Insert should still pop the main-buffer completion list"
    );
    assert!(app.editor.command_line_buffer().is_none());
    Ok(())
}

#[test]
fn test_legacy_insert_colon_command_still_works() -> Result<(), String> {
    // Checkpoint 6: the original Insert-typed `:cmd⏎` path continues to
    // dispatch through execute_input's `:`-prefix detection, unchanged by
    // the new CommandLine mode.
    let mut app = App::new()?;
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.editor.status(), "INSERT");
    app.handle_key(KeyEvent::from(KeyCode::Char(':')));
    app.handle_key(KeyEvent::from(KeyCode::Char('q')));
    assert_eq!(
        app.repl_state.input, ":q",
        "Insert-mode `:` is a literal char, not a mode shift"
    );
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.should_quit);
    Ok(())
}

#[test]
fn test_word_effect_lookup() -> Result<(), String> {
    let app = App::new()?;
    // Stack manipulation
    assert!(app.get_word_effect("dup").is_some());
    assert!(app.get_word_effect("swap").is_some());

    // Integer arithmetic - long and symbolic forms
    assert!(app.get_word_effect("i.add").is_some());
    assert!(app.get_word_effect("i.+").is_some());
    assert!(app.get_word_effect("i.multiply").is_some());
    assert!(app.get_word_effect("i.*").is_some());

    // Integer comparisons
    assert!(app.get_word_effect("i.<").is_some());
    assert!(app.get_word_effect("i.=").is_some());

    // Float arithmetic - long and symbolic forms
    assert!(app.get_word_effect("f.add").is_some());
    assert!(app.get_word_effect("f.*").is_some());

    // Float comparisons
    assert!(app.get_word_effect("f.<").is_some());

    // Unknown word
    assert!(app.get_word_effect("unknown").is_none());
    Ok(())
}

#[test]
fn test_ctrl_c_quits() -> Result<(), String> {
    let mut app = App::new()?;
    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    app.handle_key(key);
    assert!(app.should_quit);
    Ok(())
}

#[test]
fn test_tab_completion() -> Result<(), String> {
    let mut app = App::new()?;

    // Enter insert mode and type "du"
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    app.handle_key(KeyEvent::from(KeyCode::Char('d')));
    app.handle_key(KeyEvent::from(KeyCode::Char('u')));

    // Verify input and cursor are correct
    assert_eq!(app.repl_state.input, "du");
    assert_eq!(app.repl_state.cursor, 2);

    // Press Tab to trigger completion
    app.handle_key(KeyEvent::from(KeyCode::Tab));

    // Should show completions with "dup" matching
    assert!(
        app.completions.is_visible(),
        "Completions should be visible after Tab"
    );
    assert!(
        !app.completions.items().is_empty(),
        "Should have completions for 'du'"
    );
    assert!(
        app.completions.items().iter().any(|c| c.label == "dup"),
        "Should include 'dup' in completions"
    );

    Ok(())
}

#[test]
fn test_tab_completion_empty_prefix() -> Result<(), String> {
    let mut app = App::new()?;

    // Press Tab with empty input
    app.handle_key(KeyEvent::from(KeyCode::Tab));

    // Should not show completions, should show status message
    assert!(
        !app.completions.is_visible(),
        "Should not show completions for empty prefix"
    );
    assert!(
        app.status_message
            .as_ref()
            .is_some_and(|m| m.contains("type a prefix")),
        "Should show 'type a prefix' message"
    );

    Ok(())
}

#[test]
fn test_search_mode_enter_and_exit() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history
    app.repl_state
        .add_entry(HistoryEntry::new("first entry").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("second entry").with_output("2"));

    // In normal mode, '/' enters search mode
    assert!(!app.search_mode);
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    assert!(app.search_mode);
    assert!(
        app.status_message
            .as_ref()
            .is_some_and(|m| m.starts_with("/")),
        "Status should show search prompt"
    );

    // Esc exits search mode
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(!app.search_mode);

    Ok(())
}

#[test]
fn test_search_mode_filtering() -> Result<(), String> {
    let mut app = App::new()?;
    app.repl_state.history.clear(); // Clear any loaded history

    // Add history with different content
    app.repl_state
        .add_entry(HistoryEntry::new("dup swap").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("drop").with_output("2"));
    app.repl_state
        .add_entry(HistoryEntry::new("dup dup").with_output("3"));

    // Enter search mode
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    assert!(app.search_mode);

    // Type "dup" to search
    app.handle_key(KeyEvent::from(KeyCode::Char('d')));
    app.handle_key(KeyEvent::from(KeyCode::Char('u')));
    app.handle_key(KeyEvent::from(KeyCode::Char('p')));

    // Should have 2 matches (entries containing "dup")
    assert_eq!(app.search_matches.len(), 2);
    // Preview shows most recent match
    assert_eq!(app.repl_state.input, "dup dup");
    assert!(
        app.status_message
            .as_ref()
            .is_some_and(|m| m.contains("/dup") && m.contains("(1/2)")),
        "Status should show search pattern and match count"
    );

    Ok(())
}

#[test]
fn test_search_mode_accept() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history
    app.repl_state
        .add_entry(HistoryEntry::new("first").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("second").with_output("2"));

    // Enter search mode and search for "first"
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    for c in "first".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }

    // Preview should already show the match
    assert_eq!(app.repl_state.input, "first");

    // Press Enter to accept
    app.handle_key(KeyEvent::from(KeyCode::Enter));

    // Should be out of search mode with input kept
    assert!(!app.search_mode);
    assert_eq!(app.repl_state.input, "first");

    Ok(())
}

#[test]
fn test_search_mode_cancel_restores_input() -> Result<(), String> {
    let mut app = App::new()?;

    // Add history
    app.repl_state
        .add_entry(HistoryEntry::new("history entry").with_output("1"));

    // Type something first
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    for c in "my input".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.repl_state.input, "my input");

    // Enter search mode
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    for c in "history".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }

    // Preview shows match
    assert_eq!(app.repl_state.input, "history entry");

    // Cancel with Esc - should restore original
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(!app.search_mode);
    assert_eq!(app.repl_state.input, "my input");

    Ok(())
}

#[test]
fn test_search_mode_navigate_matches() -> Result<(), String> {
    let mut app = App::new()?;
    app.repl_state.history.clear(); // Clear any loaded history

    // Add history with same prefix
    app.repl_state
        .add_entry(HistoryEntry::new("test1").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("test2").with_output("2"));
    app.repl_state
        .add_entry(HistoryEntry::new("test3").with_output("3"));

    // Enter search mode and search for "test"
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    for c in "test".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }

    // Should have 3 matches, starting at index 0 (most recent first: test3)
    assert_eq!(app.search_matches.len(), 3);
    assert_eq!(app.search_match_index, 0);
    // Preview shows first match
    assert_eq!(app.repl_state.input, "test3");

    // Tab goes to next match
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.search_match_index, 1);
    assert_eq!(app.repl_state.input, "test2");

    // Shift+Tab goes to previous match
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));
    assert_eq!(app.search_match_index, 0);
    assert_eq!(app.repl_state.input, "test3");

    // Shift+Tab wraps around to last match
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));
    assert_eq!(app.search_match_index, 2);
    assert_eq!(app.repl_state.input, "test1");

    Ok(())
}

#[test]
fn test_search_mode_backspace() -> Result<(), String> {
    let mut app = App::new()?;
    app.repl_state.history.clear(); // Clear any loaded history

    // Add history
    app.repl_state
        .add_entry(HistoryEntry::new("dup").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("drop").with_output("2"));

    // Enter search mode
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));

    // Type "dup"
    for c in "dup".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    assert_eq!(app.search_pattern, "dup");
    assert_eq!(app.search_matches.len(), 1);

    // Backspace removes last char
    app.handle_key(KeyEvent::from(KeyCode::Backspace));
    assert_eq!(app.search_pattern, "du");

    // With "du", should match both "dup" (d-u-p) - wait, "drop" doesn't match "du"
    // Actually only "dup" matches "du"
    assert_eq!(app.search_matches.len(), 1);

    Ok(())
}

#[test]
fn test_search_mode_case_insensitive() -> Result<(), String> {
    let mut app = App::new()?;
    app.repl_state.history.clear(); // Clear any loaded history

    // Add history with mixed case
    app.repl_state
        .add_entry(HistoryEntry::new("DUP").with_output("1"));
    app.repl_state
        .add_entry(HistoryEntry::new("Dup").with_output("2"));
    app.repl_state
        .add_entry(HistoryEntry::new("dup").with_output("3"));

    // Enter search mode and search for lowercase "dup"
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    for c in "dup".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }

    // Should match all 3 (case insensitive)
    assert_eq!(app.search_matches.len(), 3);

    Ok(())
}

#[test]
fn test_search_not_in_insert_mode() -> Result<(), String> {
    let mut app = App::new()?;

    // Enter insert mode
    app.handle_key(KeyEvent::from(KeyCode::Char('i')));
    assert_eq!(app.editor.status(), "INSERT");

    // '/' in insert mode should insert '/', not enter search mode
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    assert!(!app.search_mode);
    assert_eq!(app.repl_state.input, "/");

    Ok(())
}
