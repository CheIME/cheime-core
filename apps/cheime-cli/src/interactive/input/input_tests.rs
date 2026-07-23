use super::*;
use crate::interactive::app::{AppState, LocalAction};
use cheime_model::{
    Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration, Key, Revision, SessionEpoch,
    SessionStatus, UiCommand,
};

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

fn composition_state(preedit: &str, status: SessionStatus) -> AppState {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(preedit, status, false));
    state
}

fn state_with_candidates() -> AppState {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Composing, true));
    state
}

fn snapshot(preedit: &str, status: SessionStatus, has_candidates: bool) -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(2),
        deployment: DeploymentGeneration::new(3),
        preedit: preedit.into(),
        cursor: preedit.len(),
        candidates: if has_candidates {
            vec![Candidate::text(CandidateId::new(4), "你", "test")]
        } else {
            vec![]
        },
        highlighted: None,
        status,
        page_size: 9,
        page: 0,
    }
}

#[test]
fn f2_and_ascii_letters_route_without_terminal_translation() {
    let state = AppState::new();
    let shifted = InputEvent {
        key: InputKey::Character('Z'),
        modifiers: InputModifiers {
            shift: true,
            ..NO_MODIFIERS
        },
        kind: InputKind::Press,
    };

    assert_eq!(
        route_key(&state, press(InputKey::F2)),
        AppAction::Local(LocalAction::ToggleDetailMode)
    );
    assert_eq!(
        route_key(&state, press(InputKey::Character('a'))),
        AppAction::Send(SessionCommand::Key(Key::Character('a')))
    );
    assert_eq!(
        route_key(&state, shifted),
        AppAction::Send(SessionCommand::Key(Key::Character('Z')))
    );
}

#[test]
fn non_ascii_and_punctuation_ignore() {
    let state = AppState::new();

    assert_eq!(
        route_key(&state, press(InputKey::Character('你'))),
        AppAction::Ignore
    );
    assert_eq!(
        route_key(&state, press(InputKey::Character('.'))),
        AppAction::Ignore
    );
}

#[test]
fn composition_sensitive_edit_keys_route_for_preedit_and_commit_pending() {
    let compositions = [
        ("ni", SessionStatus::Composing),
        ("", SessionStatus::CommitPending),
    ];

    for (preedit, status) in compositions {
        let state = composition_state(preedit, status);
        assert_eq!(
            route_key(&state, press(InputKey::Backspace)),
            AppAction::Send(SessionCommand::Key(Key::Backspace))
        );
        assert_eq!(
            route_key(&state, press(InputKey::Delete)),
            AppAction::Ignore
        );
        assert_eq!(
            route_key(&state, press(InputKey::Space)),
            AppAction::Send(SessionCommand::Key(Key::Enter))
        );
        assert_eq!(
            route_key(&state, press(InputKey::Escape)),
            AppAction::Send(SessionCommand::Key(Key::Escape))
        );
        for key in [
            InputKey::Left,
            InputKey::Right,
            InputKey::Home,
            InputKey::End,
        ] {
            assert_eq!(route_key(&state, press(key)), AppAction::Ignore);
        }
    }
}

#[test]
fn inactive_edit_keys_route_to_local_actions() {
    let state = AppState::new();
    let cases = [
        (InputKey::Backspace, LocalAction::Backspace),
        (InputKey::Delete, LocalAction::Delete),
        (InputKey::Space, LocalAction::Insert(' ')),
        (InputKey::Escape, LocalAction::ClearStatus),
        (InputKey::Left, LocalAction::MoveLeft),
        (InputKey::Right, LocalAction::MoveRight),
        (InputKey::Home, LocalAction::MoveHome),
        (InputKey::End, LocalAction::MoveEnd),
    ];

    for (key, action) in cases {
        assert_eq!(route_key(&state, press(key)), AppAction::Local(action));
    }
}

#[test]
fn inactive_snapshot_edit_keys_match_no_snapshot_local_actions() {
    let no_snapshot = AppState::new();
    let ready_snapshot = composition_state("", SessionStatus::Ready);
    let cases = [
        (InputKey::Backspace, LocalAction::Backspace),
        (InputKey::Delete, LocalAction::Delete),
        (InputKey::Space, LocalAction::Insert(' ')),
        (InputKey::Escape, LocalAction::ClearStatus),
        (InputKey::Left, LocalAction::MoveLeft),
        (InputKey::Right, LocalAction::MoveRight),
        (InputKey::Home, LocalAction::MoveHome),
        (InputKey::End, LocalAction::MoveEnd),
    ];

    for (key, action) in cases {
        let expected = AppAction::Local(action);
        assert_eq!(route_key(&no_snapshot, press(key)), expected);
        assert_eq!(route_key(&ready_snapshot, press(key)), expected);
    }
}

#[test]
fn candidate_navigation_requires_current_candidates() {
    let no_snapshot = AppState::new();
    let empty_candidate_snapshot = composition_state("ni", SessionStatus::Composing);
    let populated = state_with_candidates();

    for key in [
        InputKey::Up,
        InputKey::Down,
        InputKey::PageUp,
        InputKey::PageDown,
    ] {
        assert_eq!(route_key(&no_snapshot, press(key)), AppAction::Ignore);
        assert_eq!(
            route_key(&empty_candidate_snapshot, press(key)),
            AppAction::Ignore
        );
    }
    assert_eq!(
        route_key(&populated, press(InputKey::Up)),
        AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(-1)))
    );
    assert_eq!(
        route_key(&populated, press(InputKey::Down)),
        AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(1)))
    );
    assert_eq!(
        route_key(&populated, press(InputKey::PageUp)),
        AppAction::Send(SessionCommand::Ui(UiCommand::PreviousPage))
    );
    assert_eq!(
        route_key(&populated, press(InputKey::PageDown)),
        AppAction::Send(SessionCommand::Ui(UiCommand::NextPage))
    );
}
