use super::*;
use crate::interactive::app::{AppState, LocalAction};

const NO_MODIFIERS: InputModifiers = InputModifiers {
    shift: false,
    control: false,
    alt: false,
};

fn press(key: InputKey) -> InputEvent {
    InputEvent {
        key,
        modifiers: NO_MODIFIERS,
        kind: InputKind::Press,
    }
}

#[test]
fn repeat_and_release_ignore_every_key() {
    let state = AppState::new();
    let repeat = InputEvent {
        kind: InputKind::Repeat,
        ..press(InputKey::Backspace)
    };
    let release = InputEvent {
        kind: InputKind::Release,
        ..press(InputKey::Character('a'))
    };

    assert_eq!(route_key(&state, repeat), AppAction::Ignore);
    assert_eq!(route_key(&state, release), AppAction::Ignore);
}

#[test]
fn control_routes_exit_scroll_or_ignore() {
    let state = AppState::new();
    let control = InputModifiers {
        control: true,
        ..NO_MODIFIERS
    };
    let event = |key| InputEvent {
        key,
        modifiers: control,
        kind: InputKind::Press,
    };

    assert_eq!(
        route_key(&state, event(InputKey::Character('c'))),
        AppAction::Exit
    );
    assert_eq!(
        route_key(&state, event(InputKey::Up)),
        AppAction::Local(LocalAction::ScrollUp)
    );
    assert_eq!(
        route_key(&state, event(InputKey::Down)),
        AppAction::Local(LocalAction::ScrollDown)
    );
    for key in [
        InputKey::Character('x'),
        InputKey::F2,
        InputKey::Backspace,
        InputKey::PageDown,
    ] {
        assert_eq!(route_key(&state, event(key)), AppAction::Ignore);
    }
}

#[test]
fn alt_has_precedence_over_control_and_ignores_local_keys() {
    let state = AppState::new();
    let alt_only = |key| InputEvent {
        key,
        modifiers: InputModifiers {
            alt: true,
            ..NO_MODIFIERS
        },
        kind: InputKind::Press,
    };
    let control_alt_c = InputEvent {
        key: InputKey::Character('c'),
        modifiers: InputModifiers {
            control: true,
            alt: true,
            ..NO_MODIFIERS
        },
        kind: InputKind::Press,
    };

    assert_eq!(route_key(&state, alt_only(InputKey::F2)), AppAction::Ignore);
    assert_eq!(
        route_key(&state, alt_only(InputKey::Backspace)),
        AppAction::Ignore
    );
    assert_eq!(route_key(&state, control_alt_c), AppAction::Ignore);
}
