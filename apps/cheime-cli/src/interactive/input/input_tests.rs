use super::*;
use cheime_model::{
    Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration, Revision, SessionEpoch,
    SessionStatus,
};
use crossterm::event::{KeyEventState, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn composing_state() -> AppState {
    let mut state = AppState::new();
    state.set_snapshot(CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(2),
        deployment: DeploymentGeneration::new(1),
        preedit: String::from("ni"),
        cursor: 2,
        candidates: vec![Candidate::text(CandidateId::new(7), "你", "test")],
        highlighted: Some(CandidateId::new(7)),
        status: SessionStatus::Composing,
        page_size: 9,
        page: 0,
    });
    state
}

#[test]
fn letter_routes_directly_to_session_key() {
    assert_eq!(
        route_key(&AppState::new(), key(KeyCode::Char('n'))),
        AppAction::Send(SessionCommand::Key(Key::Character('n')))
    );
}

#[test]
fn control_c_exits_and_repeated_keys_are_ignored() {
    let mut control_c = key(KeyCode::Char('c'));
    control_c.modifiers = KeyModifiers::CONTROL;
    assert_eq!(route_key(&AppState::new(), control_c), AppAction::Exit);

    let mut repeated = key(KeyCode::Char('n'));
    repeated.kind = KeyEventKind::Repeat;
    assert_eq!(route_key(&AppState::new(), repeated), AppAction::Ignore);
}

#[test]
fn first_digit_selects_first_candidate() {
    let state = composing_state();

    assert_eq!(
        route_key(&state, key(KeyCode::Char('1'))),
        AppAction::Send(SessionCommand::Ui(UiCommand::SelectCandidate {
            epoch: SessionEpoch::new(1),
            snapshot_revision: Revision::new(2),
            candidate_id: CandidateId::new(7),
        }))
    );
}

#[test]
fn inactive_edit_keys_are_local() {
    assert_eq!(
        route_key(&AppState::new(), key(KeyCode::Backspace)),
        AppAction::Local(LocalAction::Backspace)
    );
    assert_eq!(
        route_key(&AppState::new(), key(KeyCode::Left)),
        AppAction::Local(LocalAction::MoveLeft)
    );
}

#[test]
fn composition_navigation_goes_to_session() {
    let state = composing_state();

    assert_eq!(
        route_key(&state, key(KeyCode::Down)),
        AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(1)))
    );
    assert_eq!(
        route_key(&state, key(KeyCode::PageDown)),
        AppAction::Send(SessionCommand::Ui(UiCommand::NextPage))
    );
}
