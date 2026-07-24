use super::*;
use cheime_model::{
    ActionId, CandidateSnapshot, DeploymentGeneration, PlatformAction, Revision, SessionEpoch,
};

fn snapshot(preedit: &str, status: SessionStatus) -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        deployment: DeploymentGeneration::new(1),
        preedit: preedit.into(),
        cursor: preedit.len(),
        candidates: Vec::new(),
        highlighted: None,
        status,
        page_size: 9,
        page: 0,
    }
}

#[test]
fn new_state_is_empty() {
    let state = AppState::new();

    assert_eq!(state.document().text(), "");
    assert!(state.snapshot().is_none());
    assert!(state.status().is_none());
}

#[test]
fn composition_is_derived_from_snapshot() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Composing));
    assert!(state.has_composition());

    state.set_snapshot(snapshot("", SessionStatus::Ready));
    assert!(!state.has_composition());

    state.set_snapshot(snapshot("", SessionStatus::CommitPending));
    assert!(state.has_composition());
}

#[test]
fn local_document_edits_are_blocked_while_composing() {
    let mut state = AppState::new();
    state.apply_local(LocalAction::Insert('a'));
    state.set_snapshot(snapshot("ni", SessionStatus::Composing));

    state.apply_local(LocalAction::Insert('b'));
    state.apply_local(LocalAction::Backspace);

    assert_eq!(state.document().text(), "a");
}

#[test]
fn commit_action_inserts_text_and_returns_learning_value() {
    let mut state = AppState::new();
    let action = PlatformAction {
        id: ActionId::new(1),
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        kind: PlatformActionKind::Commit {
            text: String::from("你好"),
        },
    };

    let committed = state.apply_platform_action(&action);

    assert_eq!(committed.as_deref(), Some("你好"));
    assert_eq!(state.document().text(), "你好");
}

#[test]
fn preedit_action_does_not_modify_document() {
    let mut state = AppState::new();
    let action = PlatformAction {
        id: ActionId::new(1),
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        kind: PlatformActionKind::SetPreedit {
            text: String::from("ni"),
            cursor: 2,
        },
    };

    assert_eq!(state.apply_platform_action(&action), None);
    assert_eq!(state.document().text(), "");
}
