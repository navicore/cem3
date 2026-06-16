use super::*;

#[test]
fn test_basic_motion() {
    let mut editor = VimLineEditor::new();
    let text = "hello world";

    // Move right with 'l'
    editor.handle_key(Key::char('l'), text);
    assert_eq!(editor.cursor(), 1);

    // Move right with 'w'
    editor.handle_key(Key::char('w'), text);
    assert_eq!(editor.cursor(), 6); // Start of "world"

    // Move to end with '$' - cursor should be ON the last char, not past it
    editor.handle_key(Key::char('$'), text);
    assert_eq!(editor.cursor(), 10); // 'd' is at index 10

    // Move to start with '0'
    editor.handle_key(Key::char('0'), text);
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn test_mode_switching() {
    let mut editor = VimLineEditor::new();
    let text = "hello";

    assert_eq!(editor.mode(), Mode::Normal);

    editor.handle_key(Key::char('i'), text);
    assert_eq!(editor.mode(), Mode::Insert);

    editor.handle_key(Key::code(KeyCode::Escape), text);
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_delete_word() {
    let mut editor = VimLineEditor::new();
    let text = "hello world";

    // dw should delete "hello "
    editor.handle_key(Key::char('d'), text);
    editor.handle_key(Key::char('w'), text);

    // Check we're back in Normal mode
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_insert_char() {
    let mut editor = VimLineEditor::new();
    let text = "";

    editor.handle_key(Key::char('i'), text);
    let result = editor.handle_key(Key::char('x'), text);

    assert_eq!(result.edits.len(), 1);
    match &result.edits[0] {
        TextEdit::Insert { at, text } => {
            assert_eq!(*at, 0);
            assert_eq!(text, "x");
        }
        _ => panic!("Expected Insert"),
    }
}

#[test]
fn test_visual_mode() {
    let mut editor = VimLineEditor::new();
    let text = "hello world";

    // Enter visual mode
    editor.handle_key(Key::char('v'), text);
    assert_eq!(editor.mode(), Mode::Visual);

    // Extend selection
    editor.handle_key(Key::char('w'), text);

    // Selection should cover from 0 to cursor
    let sel = editor.selection().unwrap();
    assert_eq!(sel.start, 0);
    assert!(sel.end > 0);
}

#[test]
fn test_backspace_ascii() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("abc");

    // Enter insert mode and go to end
    editor.handle_key(Key::char('i'), &text);
    editor.handle_key(Key::code(KeyCode::End), &text);
    assert_eq!(editor.cursor(), 3);

    // Backspace should delete 'c'
    let result = editor.handle_key(Key::code(KeyCode::Backspace), &text);
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, "ab");
    assert_eq!(editor.cursor(), 2);
}

#[test]
fn test_backspace_unicode() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("a😀b");

    // Enter insert mode and position after emoji (byte position 5: 1 + 4)
    editor.handle_key(Key::char('i'), &text);
    editor.handle_key(Key::code(KeyCode::End), &text);
    editor.handle_key(Key::code(KeyCode::Left), &text); // Move before 'b'
    assert_eq!(editor.cursor(), 5); // After the 4-byte emoji

    // Backspace should delete entire emoji (4 bytes), not just 1 byte
    let result = editor.handle_key(Key::code(KeyCode::Backspace), &text);
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, "ab");
    assert_eq!(editor.cursor(), 1);
}

#[test]
fn test_yank_and_paste() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("hello world");

    // Yank word with yw
    editor.handle_key(Key::char('y'), &text);
    let result = editor.handle_key(Key::char('w'), &text);
    assert!(result.yanked.is_some());
    assert_eq!(result.yanked.unwrap(), "hello ");

    // Move to end and paste
    editor.handle_key(Key::char('$'), &text);
    let result = editor.handle_key(Key::char('p'), &text);

    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, "hello worldhello ");
}

#[test]
fn test_visual_mode_delete() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("hello world");

    // Enter visual mode at position 0
    editor.handle_key(Key::char('v'), &text);
    assert_eq!(editor.mode(), Mode::Visual);

    // Extend selection with 'e' motion to end of word (stays on 'o' of hello)
    editor.handle_key(Key::char('e'), &text);

    // Delete selection with d - deletes "hello"
    let result = editor.handle_key(Key::char('d'), &text);

    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, " world");
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_operator_pending_escape() {
    let mut editor = VimLineEditor::new();
    let text = "hello world";

    // Start delete operator
    editor.handle_key(Key::char('d'), text);
    assert!(matches!(editor.mode(), Mode::OperatorPending(_)));

    // Cancel with Escape
    editor.handle_key(Key::code(KeyCode::Escape), text);
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_replace_char() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("hello");

    // Press 'r' then 'x' to replace 'h' with 'x'
    editor.handle_key(Key::char('r'), &text);
    assert_eq!(editor.mode(), Mode::ReplaceChar);

    let result = editor.handle_key(Key::char('x'), &text);
    assert_eq!(editor.mode(), Mode::Normal);

    // Apply edits
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, "xello");
}

#[test]
fn test_replace_char_escape() {
    let mut editor = VimLineEditor::new();
    let text = "hello";

    // Press 'r' then Escape should cancel
    editor.handle_key(Key::char('r'), text);
    assert_eq!(editor.mode(), Mode::ReplaceChar);

    editor.handle_key(Key::code(KeyCode::Escape), text);
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_cw_no_trailing_space() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("hello world");

    // cw should delete "hello" (not "hello ") and enter insert mode
    editor.handle_key(Key::char('c'), &text);
    let result = editor.handle_key(Key::char('w'), &text);

    assert_eq!(editor.mode(), Mode::Insert);

    // Apply edits
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    // Should preserve the space before "world"
    assert_eq!(text, " world");
}

#[test]
fn test_dw_includes_trailing_space() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("hello world");

    // dw should delete "hello " (including trailing space)
    editor.handle_key(Key::char('d'), &text);
    let result = editor.handle_key(Key::char('w'), &text);

    assert_eq!(editor.mode(), Mode::Normal);

    // Apply edits
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, "world");
}

#[test]
fn test_paste_at_empty_buffer() {
    let mut editor = VimLineEditor::new();

    // First yank something from non-empty text
    let yank_text = String::from("test");
    editor.handle_key(Key::char('y'), &yank_text);
    editor.handle_key(Key::char('w'), &yank_text);

    // Now paste into empty buffer
    let mut text = String::new();
    editor.set_cursor(0, &text);
    let result = editor.handle_key(Key::char('p'), &text);

    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, "test");
}

#[test]
fn test_dollar_cursor_on_last_char() {
    let mut editor = VimLineEditor::new();
    let text = "abc";

    // $ should place cursor ON 'c' (index 2), not past it (index 3)
    editor.handle_key(Key::char('$'), text);
    assert_eq!(editor.cursor(), 2);

    // Single character line
    let text = "x";
    editor.set_cursor(0, text);
    editor.handle_key(Key::char('$'), text);
    assert_eq!(editor.cursor(), 0); // Stay on the only char
}

#[test]
fn test_x_delete_last_char_moves_cursor_left() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("abc");

    // Move to last char
    editor.handle_key(Key::char('$'), &text);
    assert_eq!(editor.cursor(), 2); // On 'c'

    // Delete with x
    let result = editor.handle_key(Key::char('x'), &text);
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }

    assert_eq!(text, "ab");
    // Cursor should move left to stay on valid char
    assert_eq!(editor.cursor(), 1); // On 'b'
}

#[test]
fn test_x_delete_middle_char_cursor_stays() {
    let mut editor = VimLineEditor::new();
    let mut text = String::from("abc");

    // Position on 'b' (index 1)
    editor.handle_key(Key::char('l'), &text);
    assert_eq!(editor.cursor(), 1);

    // Delete with x
    let result = editor.handle_key(Key::char('x'), &text);
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }

    assert_eq!(text, "ac");
    // Cursor stays at same position (now on 'c')
    assert_eq!(editor.cursor(), 1);
}

#[test]
fn test_percent_bracket_matching() {
    let mut editor = VimLineEditor::new();
    let text = "(hello world)";

    // Cursor starts at position 0, on '('
    assert_eq!(editor.cursor(), 0);

    // Press '%' to jump to matching ')'
    editor.handle_key(Key::char('%'), text);
    assert_eq!(editor.cursor(), 12); // Position of ')'

    // Press '%' again to jump back to '('
    editor.handle_key(Key::char('%'), text);
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn test_percent_nested_brackets() {
    let mut editor = VimLineEditor::new();
    let text = "([{<>}])";

    // Start on '('
    editor.handle_key(Key::char('%'), text);
    assert_eq!(editor.cursor(), 7); // Matching ')'

    // Move to '[' at position 1
    editor.set_cursor(1, text);
    editor.handle_key(Key::char('%'), text);
    assert_eq!(editor.cursor(), 6); // Matching ']'

    // Move to '{' at position 2
    editor.set_cursor(2, text);
    editor.handle_key(Key::char('%'), text);
    assert_eq!(editor.cursor(), 5); // Matching '}'

    // Move to '<' at position 3
    editor.set_cursor(3, text);
    editor.handle_key(Key::char('%'), text);
    assert_eq!(editor.cursor(), 4); // Matching '>'
}

#[test]
fn test_percent_on_non_bracket() {
    let mut editor = VimLineEditor::new();
    let text = "hello";

    // Start at position 0 on 'h'
    let orig_cursor = editor.cursor();
    editor.handle_key(Key::char('%'), text);
    // Cursor should not move when not on a bracket
    assert_eq!(editor.cursor(), orig_cursor);
}

// ----- Mode::CommandLine (Ex-style `:` command line, opt-in) -----

#[test]
fn test_command_line_enter_clean() {
    // Checkpoint 1: flag-on `:` enters CommandLine with empty buffer; main
    // text untouched; status reports the new mode.
    let text = String::from("preserved");
    let mut editor = VimLineEditor::new().with_command_mode(true);
    editor.set_cursor(0, &text);

    let result = editor.handle_key(Key::char(':'), &text);

    assert_eq!(editor.mode(), Mode::CommandLine);
    assert_eq!(editor.command_line_buffer(), Some(""));
    assert_eq!(editor.command_line_cursor(), 0);
    assert!(result.edits.is_empty(), "no edits should touch main text");
    assert!(result.action.is_none());
    assert_eq!(editor.status(), "COMMAND");
    // Main buffer is the caller's; verify we returned no instructions to
    // mutate it.
    assert_eq!(text, "preserved");
}

#[test]
fn test_command_line_typing_submit_escape() {
    // Checkpoint 2: typing appends to the command buffer, not the main
    // buffer; Enter emits SubmitCommand without the leading `:`; Escape
    // clears the buffer with no side effects.
    let text = String::from("main");
    let mut editor = VimLineEditor::new().with_command_mode(true);

    editor.handle_key(Key::char(':'), &text);
    for c in "ir".chars() {
        let r = editor.handle_key(Key::char(c), &text);
        assert!(r.edits.is_empty(), "char '{}' must not edit main text", c);
        assert!(r.action.is_none(), "char '{}' must not emit an action", c);
    }
    assert_eq!(editor.command_line_buffer(), Some("ir"));
    assert_eq!(editor.command_line_cursor(), 2);

    let submit = editor.handle_key(Key::code(KeyCode::Enter), &text);
    match submit.action {
        Some(Action::SubmitCommand(s)) => assert_eq!(s, "ir"),
        other => panic!("expected SubmitCommand(\"ir\"), got {:?}", other),
    }
    assert!(submit.edits.is_empty(), "submit must not edit main text");
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.command_line_buffer(), None);

    // Now exercise Escape: enter again, type, escape -> Normal, no action.
    editor.handle_key(Key::char(':'), &text);
    editor.handle_key(Key::char('q'), &text);
    let esc = editor.handle_key(Key::code(KeyCode::Escape), &text);
    assert!(esc.action.is_none());
    assert!(esc.edits.is_empty());
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.command_line_buffer(), None);
    assert_eq!(text, "main");
}

#[test]
fn test_command_line_disabled_by_default() {
    // Checkpoint 3: with the flag off (default), Normal-mode `:` is a
    // no-op so other consumers of vim-line are unaffected.
    let text = String::from("hello");
    let mut editor = VimLineEditor::new();
    let result = editor.handle_key(Key::char(':'), &text);

    assert_eq!(editor.mode(), Mode::Normal);
    assert!(result.edits.is_empty());
    assert!(result.action.is_none());
    assert_eq!(editor.command_line_buffer(), None);
}

#[test]
fn test_command_line_does_not_steal_r_replace() {
    // Checkpoint 4: `r:` (replace char with `:`) still works with the
    // command-mode flag enabled — ReplaceChar swallows the next key
    // before Normal-mode dispatch can see it.
    let mut text = String::from("abc");
    let mut editor = VimLineEditor::new().with_command_mode(true);

    editor.handle_key(Key::char('r'), &text);
    assert_eq!(editor.mode(), Mode::ReplaceChar);
    let result = editor.handle_key(Key::char(':'), &text);
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, ":bc");
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn test_command_line_does_not_steal_d_operator() {
    // Checkpoint 4 (cont.): `d:` cancels the delete operator like before
    // (`:` is not a known motion), without entering CommandLine.
    let text = String::from("hello");
    let mut editor = VimLineEditor::new().with_command_mode(true);

    editor.handle_key(Key::char('d'), &text);
    assert!(matches!(editor.mode(), Mode::OperatorPending(_)));
    let result = editor.handle_key(Key::char(':'), &text);
    assert_eq!(editor.mode(), Mode::Normal);
    assert!(result.edits.is_empty());
    assert!(result.action.is_none());
    assert_eq!(editor.command_line_buffer(), None);
}

#[test]
fn test_command_line_insert_mode_literal_colon_unchanged() {
    // Checkpoint 4 (cont.): Insert-mode `:` is still a literal char insert
    // even with the flag on. This is the path word definitions (`: foo …`)
    // flow through.
    let mut text = String::new();
    let mut editor = VimLineEditor::new().with_command_mode(true);

    editor.handle_key(Key::char('i'), &text);
    let result = editor.handle_key(Key::char(':'), &text);
    for edit in result.edits.into_iter().rev() {
        edit.apply(&mut text);
    }
    assert_eq!(text, ":");
    assert_eq!(editor.mode(), Mode::Insert);
}

#[test]
fn test_command_line_backspace_and_arrows() {
    // Backspace pops from the command buffer; arrows move the command
    // cursor; mid-buffer typing inserts at the cursor — none of which
    // should touch the host's main text.
    let text = String::from("ignored");
    let mut editor = VimLineEditor::new().with_command_mode(true);

    editor.handle_key(Key::char(':'), &text);
    for c in "abd".chars() {
        editor.handle_key(Key::char(c), &text);
    }
    assert_eq!(editor.command_line_buffer(), Some("abd"));

    editor.handle_key(Key::code(KeyCode::Backspace), &text);
    assert_eq!(editor.command_line_buffer(), Some("ab"));
    assert_eq!(editor.command_line_cursor(), 2);

    editor.handle_key(Key::code(KeyCode::Left), &text);
    assert_eq!(editor.command_line_cursor(), 1);
    editor.handle_key(Key::char('X'), &text);
    assert_eq!(editor.command_line_buffer(), Some("aXb"));

    editor.handle_key(Key::code(KeyCode::Home), &text);
    assert_eq!(editor.command_line_cursor(), 0);
    editor.handle_key(Key::code(KeyCode::End), &text);
    assert_eq!(editor.command_line_cursor(), 3);

    assert_eq!(text, "ignored");
}

#[test]
fn normal_k_on_single_line_emits_history_prev() {
    let mut editor = VimLineEditor::new();
    let text = "hello";
    let result = editor.handle_key(Key::char('k'), text);
    assert_eq!(result.action, Some(Action::HistoryPrev));
}

#[test]
fn normal_j_on_single_line_emits_history_next() {
    let mut editor = VimLineEditor::new();
    let text = "hello";
    let result = editor.handle_key(Key::char('j'), text);
    assert_eq!(result.action, Some(Action::HistoryNext));
}

#[test]
fn normal_kj_in_middle_of_multiline_moves_between_lines() {
    let mut editor = VimLineEditor::new();
    let text = "alpha\nbeta\ngamma";

    // Cursor on the middle line ('b' of "beta", index 6).
    editor.set_cursor(6, text);
    let result = editor.handle_key(Key::char('k'), text);
    assert!(
        result.action.is_none(),
        "k off-boundary is a motion, not history"
    );
    assert_eq!(editor.cursor(), 0, "moved up to 'a' of \"alpha\"");

    // From "alpha", k again IS at the first line → history.
    let result = editor.handle_key(Key::char('k'), text);
    assert_eq!(result.action, Some(Action::HistoryPrev));

    // Cursor back to middle line, then j off-boundary moves down.
    editor.set_cursor(6, text);
    let result = editor.handle_key(Key::char('j'), text);
    assert!(
        result.action.is_none(),
        "j off-boundary is a motion, not history"
    );
    assert_eq!(editor.cursor(), 11, "moved down to 'g' of \"gamma\"");

    // From "gamma", j again IS at the last line → history.
    let result = editor.handle_key(Key::char('j'), text);
    assert_eq!(result.action, Some(Action::HistoryNext));
}

#[test]
fn normal_arrows_still_emit_history_unchanged() {
    let mut editor = VimLineEditor::new();
    let text = "alpha\nbeta\ngamma";
    editor.set_cursor(6, text);
    // Up/Down arrows in Normal mode are always history nav regardless of
    // line position — preserving existing behavior.
    let up = editor.handle_key(Key::code(KeyCode::Up), text);
    assert_eq!(up.action, Some(Action::HistoryPrev));
    let down = editor.handle_key(Key::code(KeyCode::Down), text);
    assert_eq!(down.action, Some(Action::HistoryNext));
}

#[test]
fn visual_kj_remains_motion_only() {
    let mut editor = VimLineEditor::new();
    let text = "one\ntwo";
    editor.handle_key(Key::char('v'), text);
    // In Visual mode, j/k still move the cursor and never emit a history
    // intent — only Normal mode has the boundary fall-through.
    let result = editor.handle_key(Key::char('j'), text);
    assert!(result.action.is_none());
    assert_eq!(editor.cursor(), 4);
}
