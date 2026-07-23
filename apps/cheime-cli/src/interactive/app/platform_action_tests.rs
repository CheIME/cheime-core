use super::*;
use cheime_model::{
    ActionId, Candidate, CandidateId, DeploymentGeneration, PlatformAction, PlatformActionKind,
    Revision, SessionEpoch, SessionStatus,
};

fn action(kind: PlatformActionKind) -> PlatformAction {
    PlatformAction {
        id: ActionId::new(9),
        epoch: SessionEpoch::new(3),
        revision: Revision::new(7),
        kind,
    }
}

fn pending_snapshot() -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(3),
        revision: Revision::new(7),
        deployment: cheime_model::DeploymentGeneration::new(1),
        preedit: "ni".into(),
        cursor: 2,
        candidates: vec![],
        highlighted: None,
        status: SessionStatus::CommitPending,
        page_size: 9,
        page: 0,
    }
}

fn snapshot_to_preserve() -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(41),
        revision: Revision::new(73),
        deployment: DeploymentGeneration::new(17),
        preedit: "persisted-preedit".into(),
        cursor: 9,
        candidates: vec![Candidate::text(CandidateId::new(23), "候选", "preserved")],
        highlighted: Some(CandidateId::new(23)),
        status: SessionStatus::Composing,
        page_size: 5,
        page: 2,
    }
}

#[test]
fn commit_inserts_once_at_start_while_commit_is_pending() {
    // Given
    let mut state = AppState::new();
    state.document.insert("你a好");
    state.document.move_home();
    state.set_snapshot(pending_snapshot());
    let action = action(PlatformActionKind::Commit {
        text: "界😀".into(),
    });
    let expected = action.clone();

    // When
    let outcome = state.apply_platform_action(action);

    // Then
    assert_eq!(
        outcome,
        PlatformActionApplication::Committed {
            action: expected,
            text: "界😀".into(),
        }
    );
    assert_eq!(state.document().text(), "界😀你a好");
    assert_eq!(state.document().cursor(), 7);
    assert!(
        state
            .document()
            .text()
            .is_char_boundary(state.document().cursor())
    );
}

#[test]
fn commit_inserts_once_before_a_multibyte_scalar_in_the_middle() {
    // Given
    let mut state = AppState::new();
    state.document.insert("a你好b");
    state.document.move_home();
    state.document.move_right();
    let action = action(PlatformActionKind::Commit { text: "界".into() });

    // When
    let outcome = state.apply_platform_action(action.clone());

    // Then
    assert_eq!(
        outcome,
        PlatformActionApplication::Committed {
            action,
            text: "界".into(),
        }
    );
    assert_eq!(state.document().text(), "a界你好b");
    assert_eq!(state.document().cursor(), 4);
    assert!(
        state
            .document()
            .text()
            .is_char_boundary(state.document().cursor())
    );
}

#[test]
fn commit_inserts_once_after_a_multibyte_scalar_in_the_middle() {
    // Given
    let mut state = AppState::new();
    state.document.insert("a你好b");
    state.document.move_home();
    state.document.move_right();
    state.document.move_right();
    let action = action(PlatformActionKind::Commit { text: "界".into() });

    // When
    let outcome = state.apply_platform_action(action.clone());

    // Then
    assert_eq!(
        outcome,
        PlatformActionApplication::Committed {
            action,
            text: "界".into(),
        }
    );
    assert_eq!(state.document().text(), "a你界好b");
    assert_eq!(state.document().cursor(), 7);
    assert!(
        state
            .document()
            .text()
            .is_char_boundary(state.document().cursor())
    );
}

#[test]
fn commit_inserts_once_at_end() {
    // Given
    let mut state = AppState::new();
    state.document.insert("a你好b");
    let action = action(PlatformActionKind::Commit { text: "界".into() });

    // When
    let outcome = state.apply_platform_action(action.clone());

    // Then
    assert_eq!(
        outcome,
        PlatformActionApplication::Committed {
            action,
            text: "界".into(),
        }
    );
    assert_eq!(state.document().text(), "a你好b界");
    assert_eq!(state.document().cursor(), 11);
    assert!(
        state
            .document()
            .text()
            .is_char_boundary(state.document().cursor())
    );
}

#[test]
fn set_preedit_preserves_seeded_document_and_cursor() {
    // Given
    let mut state = AppState::new();
    state.document.insert("a你b");
    state.document.move_left();
    let snapshot = snapshot_to_preserve();
    state.set_snapshot(snapshot.clone());
    let action = action(PlatformActionKind::SetPreedit {
        text: "ni".into(),
        cursor: 2,
    });

    // When
    let outcome = state.apply_platform_action(action.clone());

    // Then
    assert_eq!(
        outcome,
        PlatformActionApplication::NoDocumentChange { action }
    );
    assert_eq!(state.document().text(), "a你b");
    assert_eq!(state.document().cursor(), 4);
    assert_eq!(state.snapshot(), Some(&snapshot));
}

#[test]
fn cancel_composition_preserves_seeded_document_and_cursor() {
    // Given
    let mut state = AppState::new();
    state.document.insert("a你b");
    state.document.move_left();
    let snapshot = snapshot_to_preserve();
    state.set_snapshot(snapshot.clone());
    let action = action(PlatformActionKind::CancelComposition);

    // When
    let outcome = state.apply_platform_action(action.clone());

    // Then
    assert_eq!(
        outcome,
        PlatformActionApplication::NoDocumentChange { action }
    );
    assert_eq!(state.document().text(), "a你b");
    assert_eq!(state.document().cursor(), 4);
    assert_eq!(state.snapshot(), Some(&snapshot));
}
