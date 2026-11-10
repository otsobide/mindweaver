//! The two pieces of the screen's state every form and page share: the line
//! a person types into (`Input`) and the page a person reads (`Reader`),
//! each held key by key with no screen around it. What the screens do with
//! them is `tests/tui_test.rs`'s, and what a terminal sends them
//! `tests/tui_e2e_test.rs`'s.

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::from(code)
}

fn held(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

/// Press each key in turn, and say whether every one was an edit.
fn edit(input: &mut Input, keys: &[KeyEvent]) -> bool {
    keys.iter().all(|key| input.press(*key))
}

// ---------------------------------------------------------------------------
// The line a person types into
// ---------------------------------------------------------------------------

#[test]
fn an_input_starts_with_its_cursor_after_the_last_character_counted_in_characters() {
    let input = Input::with("café 👍");

    assert_eq!(input.text(), "café 👍");
    // Six characters, ten bytes: a cursor counted in bytes would sit inside
    // the thumb.
    assert_eq!(input.cursor(), 6);
    assert_eq!(Input::default().cursor(), 0);
}

// Each of these puts characters of different widths before the cursor, two
// bytes and four, so a cursor read as a byte offset lands on another
// character rather than on the same one by chance.

#[test]
fn a_character_typed_lands_at_the_cursor_even_after_ones_wider_than_a_byte() {
    let mut input = Input::with("ñé👍a");

    let edited = edit(
        &mut input,
        &[
            key(KeyCode::Home),
            key(KeyCode::Right),
            key(KeyCode::Right),
            key(KeyCode::Right),
            key(KeyCode::Char('X')),
        ],
    );

    assert!(edited);
    assert_eq!(input.text(), "ñé👍Xa");
    assert_eq!(input.cursor(), 4);
}

#[test]
fn backspace_takes_the_whole_character_before_the_cursor() {
    let mut input = Input::with("ñé👍a");

    edit(&mut input, &[key(KeyCode::Left), key(KeyCode::Backspace)]);

    assert_eq!(input.text(), "ñéa");
    assert_eq!(input.cursor(), 2);
}

#[test]
fn delete_takes_the_whole_character_under_the_cursor_and_leaves_it_where_it_was() {
    let mut input = Input::with("ñé👍a");

    edit(
        &mut input,
        &[
            key(KeyCode::Home),
            key(KeyCode::Right),
            key(KeyCode::Right),
            key(KeyCode::Delete),
        ],
    );

    assert_eq!(input.text(), "ñéa");
    assert_eq!(input.cursor(), 2);
}

#[test]
fn backspace_at_the_start_and_delete_at_the_end_are_no_edit_and_change_nothing() {
    let mut input = Input::with("ab");

    // Not an edit, so a form may take the key as one of its own.
    assert!(!input.press(key(KeyCode::Delete)));
    input.press(key(KeyCode::Home));
    assert!(!input.press(key(KeyCode::Backspace)));

    assert_eq!(input.text(), "ab");
    assert_eq!(input.cursor(), 0);
}

#[test]
fn the_cursor_stops_at_either_end_and_home_and_end_reach_them() {
    let mut input = Input::with("añb");

    edit(&mut input, &[key(KeyCode::Right), key(KeyCode::Right)]);
    assert_eq!(input.cursor(), 3);
    edit(&mut input, &[key(KeyCode::Home), key(KeyCode::Left)]);
    assert_eq!(input.cursor(), 0);
    edit(&mut input, &[key(KeyCode::End)]);
    assert_eq!(input.cursor(), 3);
    assert_eq!(input.text(), "añb");
}

#[test]
fn ctrl_u_clears_the_line_wherever_the_cursor_is() {
    let mut input = Input::with("mind -C añb");
    input.press(key(KeyCode::Left));
    input.press(key(KeyCode::Left));

    assert!(input.press(held(KeyCode::Char('u'), KeyModifiers::CONTROL)));

    assert_eq!(input, Input::default());
}

#[test]
fn a_letter_held_with_ctrl_or_alt_is_not_typed() {
    let mut input = Input::with("ab");

    // Alt with a letter is what a terminal sends for an escape pressed just
    // before it: typed, it would put the next key in the line instead of
    // leaving the form.
    assert!(!input.press(held(KeyCode::Char('q'), KeyModifiers::ALT)));
    assert!(!input.press(held(KeyCode::Char('a'), KeyModifiers::CONTROL)));

    assert_eq!(input, Input::with("ab"));
}

#[test]
fn a_key_that_edits_no_line_is_said_to_be_none() {
    let mut input = Input::with("ab");

    for code in [
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Up,
        KeyCode::Down,
    ] {
        assert!(!input.press(key(code)), "{code:?}");
    }
    assert_eq!(input, Input::with("ab"));
}

// ---------------------------------------------------------------------------
// The page a person reads
// ---------------------------------------------------------------------------

/// A page of `count` lines, read from its first.
fn page(count: usize) -> Reader {
    Reader {
        title: String::from("a page"),
        lines: (0..count)
            .map(|line| (Tone::Plain, format!("line {line}")))
            .collect(),
        scroll: 0,
        searches: false,
        writes: None,
    }
}

/// Where `reader` is scrolled to once `keys` are pressed, each read as the
/// screen reads it; `None` once one of them leaves the page.
fn scrolled(mut reader: Reader, keys: &[KeyCode]) -> Option<usize> {
    for code in keys {
        match reading(reader, key(*code)) {
            (Mode::Reading(next), Step::Stay) => reader = next,
            (Mode::Browse, Step::Stay) => return None,
            (mode, step) => panic!("{code:?} went to {mode:?} and asked for {step:?}"),
        }
    }
    Some(reader.scroll)
}

#[test]
fn a_page_scrolls_a_line_at_a_time_and_never_above_its_first() {
    assert_eq!(scrolled(page(25), &[KeyCode::Down]), Some(1));
    assert_eq!(scrolled(page(25), &[KeyCode::Char('j')]), Some(1));
    assert_eq!(
        scrolled(page(25), &[KeyCode::Down, KeyCode::Down, KeyCode::Up]),
        Some(1)
    );
    assert_eq!(
        scrolled(page(25), &[KeyCode::Char('j'), KeyCode::Char('k')]),
        Some(0)
    );
    assert_eq!(scrolled(page(25), &[KeyCode::Up]), Some(0));
}

#[test]
fn a_page_scrolls_ten_lines_at_a_time_and_stops_at_its_last() {
    assert_eq!(scrolled(page(25), &[KeyCode::PageDown]), Some(10));
    assert_eq!(scrolled(page(25), &[KeyCode::Char(' ')]), Some(10));
    assert_eq!(
        scrolled(
            page(25),
            &[KeyCode::PageDown, KeyCode::PageDown, KeyCode::PageDown]
        ),
        Some(24)
    );
    assert_eq!(
        scrolled(
            page(25),
            &[KeyCode::PageDown, KeyCode::PageDown, KeyCode::PageUp]
        ),
        Some(10)
    );
    assert_eq!(
        scrolled(
            page(25),
            &[KeyCode::PageDown, KeyCode::PageUp, KeyCode::PageUp]
        ),
        Some(0)
    );
}

#[test]
fn a_page_goes_to_either_end_at_once() {
    assert_eq!(scrolled(page(25), &[KeyCode::End]), Some(24));
    assert_eq!(scrolled(page(25), &[KeyCode::Char('G')]), Some(24));
    assert_eq!(
        scrolled(page(25), &[KeyCode::Char('G'), KeyCode::Char('g')]),
        Some(0)
    );
    assert_eq!(scrolled(page(25), &[KeyCode::End, KeyCode::Home]), Some(0));
    // Past its last line there is nothing, so a page of one never moves.
    assert_eq!(
        scrolled(page(1), &[KeyCode::End, KeyCode::Down, KeyCode::PageDown]),
        Some(0)
    );
}

#[test]
fn q_esc_and_enter_leave_a_page_and_any_other_key_keeps_it_where_it_was() {
    for code in [KeyCode::Char('q'), KeyCode::Esc, KeyCode::Enter] {
        assert_eq!(scrolled(page(25), &[KeyCode::Down, code]), None, "{code:?}");
    }
    assert_eq!(
        scrolled(page(25), &[KeyCode::Down, KeyCode::Char('x'), KeyCode::Tab]),
        Some(1)
    );
}

#[test]
fn a_page_of_what_ran_says_its_output_then_its_warnings_then_why_it_failed() {
    let ran = Done {
        command: String::from("mind -C collapse update --yes"),
        stdout: String::from("updated commit-style in collapse\n"),
        stderr: vec![String::from("warning: deploy was left alone")],
        error: Some(String::from("1 copy was left as it is")),
        ok: false,
    };
    let refused = Done::failed(String::from("mind -C collapse share --yes shared"), "busy");

    let reader = Reader::of(&[ran, refused]);

    assert_eq!(
        reader.title,
        "mind -C collapse update --yes && mind -C collapse share --yes shared"
    );
    assert_eq!(
        reader.lines,
        [
            (
                Tone::Plain,
                String::from("updated commit-style in collapse")
            ),
            (Tone::Warn, String::from("warning: deploy was left alone")),
            (Tone::Bad, String::from("error: 1 copy was left as it is")),
            (Tone::Bad, String::from("error: busy")),
        ]
    );
    assert_eq!(reader.scroll, 0);
}

#[test]
fn a_page_of_what_said_nothing_says_so_rather_than_showing_nothing() {
    let quiet = Done {
        command: String::from("mindweaver gather list"),
        stdout: String::new(),
        stderr: Vec::new(),
        error: None,
        ok: true,
    };

    let reader = Reader::of(&[quiet]);

    assert_eq!(
        reader.lines,
        [(Tone::Info, String::from("nothing to show"))]
    );
}
