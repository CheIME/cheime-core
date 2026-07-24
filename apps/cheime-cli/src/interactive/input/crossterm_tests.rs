use super::{InputEvent, InputKey, InputKind, InputModifiers, from_crossterm_key};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

// ── helpers ─────────────────────────────────────────────────────────────────

fn ct_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn no_mods() -> KeyModifiers {
    KeyModifiers::NONE
}

// ── basic key mapping ───────────────────────────────────────────────────────

#[test]
fn maps_character_key() {
    let result = from_crossterm_key(ct_key(KeyCode::Char('a'), no_mods()));
    assert_eq!(
        result,
        Some(InputEvent {
            key: InputKey::Character('a'),
            modifiers: InputModifiers {
                shift: false,
                control: false,
                alt: false
            },
            kind: InputKind::Press,
        })
    );
}

#[test]
fn maps_backspace() {
    let result = from_crossterm_key(ct_key(KeyCode::Backspace, no_mods()));
    assert_eq!(result.unwrap().key, InputKey::Backspace);
}

#[test]
fn maps_enter() {
    let result = from_crossterm_key(ct_key(KeyCode::Enter, no_mods()));
    assert_eq!(result.unwrap().key, InputKey::Enter);
}

#[test]
fn maps_escape() {
    let result = from_crossterm_key(ct_key(KeyCode::Esc, no_mods()));
    assert_eq!(result.unwrap().key, InputKey::Escape);
}

#[test]
fn maps_arrow_keys() {
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::Left, no_mods()))
            .unwrap()
            .key,
        InputKey::Left,
    );
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::Right, no_mods()))
            .unwrap()
            .key,
        InputKey::Right,
    );
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::Up, no_mods()))
            .unwrap()
            .key,
        InputKey::Up,
    );
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::Down, no_mods()))
            .unwrap()
            .key,
        InputKey::Down,
    );
}

#[test]
fn maps_navigation_keys() {
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::Home, no_mods()))
            .unwrap()
            .key,
        InputKey::Home,
    );
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::End, no_mods()))
            .unwrap()
            .key,
        InputKey::End,
    );
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::PageUp, no_mods()))
            .unwrap()
            .key,
        InputKey::PageUp,
    );
    assert_eq!(
        from_crossterm_key(ct_key(KeyCode::PageDown, no_mods()))
            .unwrap()
            .key,
        InputKey::PageDown,
    );
}

#[test]
fn maps_f2() {
    let result = from_crossterm_key(ct_key(KeyCode::F(2), no_mods()));
    assert_eq!(result.unwrap().key, InputKey::F2);
}

#[test]
fn maps_space_via_char() {
    let result = from_crossterm_key(ct_key(KeyCode::Char(' '), no_mods()));
    assert_eq!(result.unwrap().key, InputKey::Space);
}

// ── modifiers ───────────────────────────────────────────────────────────────

#[test]
fn captures_control_modifier() {
    let result = from_crossterm_key(ct_key(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(
        result.unwrap().modifiers,
        InputModifiers {
            shift: false,
            control: true,
            alt: false,
        }
    );
}

#[test]
fn captures_shift_modifier() {
    let result = from_crossterm_key(ct_key(KeyCode::Char('A'), KeyModifiers::SHIFT));
    assert!(result.unwrap().modifiers.shift);
}

#[test]
fn captures_alt_modifier() {
    let result = from_crossterm_key(ct_key(KeyCode::Char('x'), KeyModifiers::ALT));
    assert!(result.unwrap().modifiers.alt);
}

// ── event kind ──────────────────────────────────────────────────────────────

#[test]
fn maps_press_kind() {
    let ct = KeyEvent::new(KeyCode::Char('p'), no_mods());
    // KeyEvent::new defaults to Press
    assert_eq!(from_crossterm_key(ct).unwrap().kind, InputKind::Press);
}

#[test]
fn maps_repeat_kind() {
    let ct = KeyEvent::new_with_kind(KeyCode::Char('r'), no_mods(), KeyEventKind::Repeat);
    assert_eq!(from_crossterm_key(ct).unwrap().kind, InputKind::Repeat);
}

#[test]
fn maps_release_kind() {
    let ct = KeyEvent::new_with_kind(KeyCode::Char('r'), no_mods(), KeyEventKind::Release);
    assert_eq!(from_crossterm_key(ct).unwrap().kind, InputKind::Release);
}

// ── unmapped keys ──────────────────────────────────────────────────────────

#[test]
fn returns_none_for_f3() {
    assert_eq!(from_crossterm_key(ct_key(KeyCode::F(3), no_mods())), None);
}

#[test]
fn returns_none_for_f1() {
    assert_eq!(from_crossterm_key(ct_key(KeyCode::F(1), no_mods())), None);
}

#[test]
fn returns_none_for_tab() {
    assert_eq!(from_crossterm_key(ct_key(KeyCode::Tab, no_mods())), None);
}

#[test]
fn returns_none_for_null() {
    assert_eq!(from_crossterm_key(ct_key(KeyCode::Null, no_mods())), None);
}

#[test]
fn returns_none_for_insert() {
    assert_eq!(from_crossterm_key(ct_key(KeyCode::Insert, no_mods())), None);
}
