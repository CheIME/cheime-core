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

struct SnapshotFixture<'a> {
    epoch: u64,
    revision: u64,
    preedit: &'a str,
    candidate_ids: &'a [u64],
    highlighted: Option<u64>,
    status: SessionStatus,
}

fn press(key: InputKey) -> InputEvent {
    InputEvent {
        key,
        modifiers: NO_MODIFIERS,
        kind: InputKind::Press,
    }
}

fn snapshot(fixture: SnapshotFixture<'_>) -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(fixture.epoch),
        revision: Revision::new(fixture.revision),
        deployment: DeploymentGeneration::new(1),
        preedit: fixture.preedit.into(),
        cursor: fixture.preedit.len(),
        candidates: fixture
            .candidate_ids
            .iter()
            .map(|&id| Candidate::text(CandidateId::new(id), "candidate", "test"))
            .collect(),
        highlighted: fixture.highlighted.map(CandidateId::new),
        status: fixture.status,
        page_size: 9,
        page: 0,
    }
}

fn selection(epoch: u64, revision: u64, candidate_id: u64) -> AppAction {
    AppAction::Send(SessionCommand::Ui(UiCommand::SelectCandidate {
        epoch: SessionEpoch::new(epoch),
        snapshot_revision: Revision::new(revision),
        candidate_id: CandidateId::new(candidate_id),
    }))
}

#[test]
fn enter_selects_highlighted_current_snapshot_identity() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 23,
        revision: 29,
        preedit: "ni",
        candidate_ids: &[211, 223],
        highlighted: Some(223),
        status: SessionStatus::Composing,
    }));

    let action = route_key(&state, press(InputKey::Enter));

    assert_eq!(action, selection(23, 29, 223));
}

#[test]
fn enter_after_snapshot_replacement_selects_latest_identity() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 11,
        revision: 17,
        preedit: "old",
        candidate_ids: &[101],
        highlighted: Some(101),
        status: SessionStatus::Composing,
    }));
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 31,
        revision: 37,
        preedit: "new",
        candidate_ids: &[307],
        highlighted: Some(307),
        status: SessionStatus::Composing,
    }));

    let action = route_key(&state, press(InputKey::Enter));

    assert_eq!(action, selection(31, 37, 307));
}

#[test]
fn enter_without_highlight_sets_nonfatal_status() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 41,
        revision: 43,
        preedit: "ni",
        candidate_ids: &[401],
        highlighted: None,
        status: SessionStatus::Composing,
    }));

    let action = route_key(&state, press(InputKey::Enter));

    assert_eq!(
        action,
        AppAction::Local(LocalAction::SetStatus(NO_HIGHLIGHT_STATUS))
    );
    match action {
        AppAction::Local(local) => state.apply_local(local),
        AppAction::Send(_) | AppAction::Exit | AppAction::Ignore => {
            panic!("highlight-less Enter must be local")
        }
    }
    assert_eq!(state.status(), Some(NO_HIGHLIGHT_STATUS));
}

#[test]
fn enter_sends_raw_commit_for_current_nonempty_preedit_without_candidates() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 47,
        revision: 53,
        preedit: "raw",
        candidate_ids: &[],
        highlighted: None,
        status: SessionStatus::Composing,
    }));

    let action = route_key(&state, press(InputKey::Enter));

    assert_eq!(action, AppAction::Send(SessionCommand::Key(Key::Enter)));
}

#[test]
fn enter_ignores_when_no_current_selection_or_preedit() {
    let no_snapshot = AppState::new();
    let statuses = [
        SessionStatus::Ready,
        SessionStatus::Transparent,
        SessionStatus::CommitPending,
    ];

    assert_eq!(
        route_key(&no_snapshot, press(InputKey::Enter)),
        AppAction::Ignore
    );
    for status in statuses {
        let mut state = AppState::new();
        state.set_snapshot(snapshot(SnapshotFixture {
            epoch: 59,
            revision: 61,
            preedit: "",
            candidate_ids: &[],
            highlighted: None,
            status,
        }));

        assert_eq!(route_key(&state, press(InputKey::Enter)), AppAction::Ignore);
    }
}

#[test]
fn digits_select_first_middle_and_ninth_candidates_from_current_page() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 67,
        revision: 71,
        preedit: "ni",
        candidate_ids: &[701, 709, 719, 727, 739, 743, 751, 761, 773],
        highlighted: None,
        status: SessionStatus::Composing,
    }));

    for (digit, candidate_id) in [('1', 701), ('5', 739), ('9', 773)] {
        assert_eq!(
            route_key(&state, press(InputKey::Character(digit))),
            selection(67, 71, candidate_id)
        );
    }
}

#[test]
fn absent_digit_sends_core_character_during_nonempty_preedit() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 79,
        revision: 83,
        preedit: "ni",
        candidate_ids: &[809],
        highlighted: None,
        status: SessionStatus::Composing,
    }));

    let action = route_key(&state, press(InputKey::Character('2')));

    assert_eq!(
        action,
        AppAction::Send(SessionCommand::Key(Key::Character('2')))
    );
}

#[test]
fn absent_digit_sends_core_character_during_commit_pending() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 89,
        revision: 97,
        preedit: "",
        candidate_ids: &[],
        highlighted: None,
        status: SessionStatus::CommitPending,
    }));

    let action = route_key(&state, press(InputKey::Character('7')));

    assert_eq!(
        action,
        AppAction::Send(SessionCommand::Key(Key::Character('7')))
    );
}

#[test]
fn absent_digit_inserts_locally_without_composition() {
    let no_snapshot = AppState::new();
    let mut ready_snapshot = AppState::new();
    ready_snapshot.set_snapshot(snapshot(SnapshotFixture {
        epoch: 101,
        revision: 103,
        preedit: "",
        candidate_ids: &[],
        highlighted: None,
        status: SessionStatus::Ready,
    }));

    assert_eq!(
        route_key(&no_snapshot, press(InputKey::Character('4'))),
        AppAction::Local(LocalAction::Insert('4'))
    );
    assert_eq!(
        route_key(&ready_snapshot, press(InputKey::Character('4'))),
        AppAction::Local(LocalAction::Insert('4'))
    );
}

#[test]
fn digit_after_snapshot_replacement_selects_latest_identity() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 107,
        revision: 109,
        preedit: "old",
        candidate_ids: &[1103],
        highlighted: Some(1103),
        status: SessionStatus::Composing,
    }));
    state.set_snapshot(snapshot(SnapshotFixture {
        epoch: 113,
        revision: 127,
        preedit: "new",
        candidate_ids: &[1201],
        highlighted: Some(1201),
        status: SessionStatus::Composing,
    }));

    let action = route_key(&state, press(InputKey::Character('1')));

    assert_eq!(action, selection(113, 127, 1201));
}
