use super::*;
use cheime_model::{
    Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration, Revision, SessionEpoch,
};

// ── helpers ───────────────────────────────────────────────────────

fn snapshot(preedit: &str, status: SessionStatus) -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        deployment: DeploymentGeneration::new(1),
        preedit: preedit.into(),
        cursor: preedit.len(),
        candidates: vec![],
        highlighted: None,
        status,
        page_size: 9,
        page: 0,
    }
}

fn snapshot_with_candidates(preedit: &str, status: SessionStatus) -> CandidateSnapshot {
    CandidateSnapshot {
        candidates: vec![
            Candidate::text(CandidateId::new(1), "你", "test"),
            Candidate::text(CandidateId::new(2), "拟", "test"),
        ],
        ..snapshot(preedit, status)
    }
}

// ── initial state ─────────────────────────────────────────────────

#[test]
fn initial_state_has_empty_document() {
    let state = AppState::new();
    assert_eq!(state.document().text(), "");
    assert_eq!(state.document().cursor(), 0);
}

#[test]
fn initial_state_parsed_mode_and_no_scroll() {
    let state = AppState::new();
    assert!(matches!(state.detail_mode(), DetailMode::Parsed));
    assert_eq!(state.detail_scroll(), 0);
}

#[test]
fn initial_state_no_status_no_snapshot_not_exiting() {
    let state = AppState::new();
    assert!(state.status().is_none());
    assert!(state.snapshot().is_none());
    assert!(!state.should_exit());
}

// ── has_composition ───────────────────────────────────────────────

#[test]
fn has_composition_false_when_no_snapshot() {
    let state = AppState::new();
    assert!(!state.has_composition());
}

#[test]
fn has_composition_false_when_empty_preedit_ready() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("", SessionStatus::Ready));
    assert!(!state.has_composition());
}

#[test]
fn has_composition_true_when_nonempty_preedit_ready() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Ready));
    assert!(state.has_composition());
}

#[test]
fn has_composition_true_when_nonempty_preedit_composing() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("wo", SessionStatus::Composing));
    assert!(state.has_composition());
}

#[test]
fn has_composition_true_when_empty_preedit_commit_pending() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("", SessionStatus::CommitPending));
    assert!(state.has_composition());
}

// ── commit_pending ────────────────────────────────────────────────

#[test]
fn commit_pending_false_when_no_snapshot() {
    let state = AppState::new();
    assert!(!state.commit_pending());
}

#[test]
fn commit_pending_false_when_ready() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Ready));
    assert!(!state.commit_pending());
}

#[test]
fn commit_pending_false_when_composing() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Composing));
    assert!(!state.commit_pending());
}

#[test]
fn commit_pending_true_when_commit_pending_status() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::CommitPending));
    assert!(state.commit_pending());
}

// ── Transparent status predicates ─────────────────────────────────

#[test]
fn has_composition_false_when_transparent_with_empty_preedit() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("", SessionStatus::Transparent));
    assert!(!state.has_composition());
}

#[test]
fn has_composition_true_when_transparent_with_nonempty_preedit() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Transparent));
    assert!(state.has_composition());
}

#[test]
fn commit_pending_false_when_transparent() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Transparent));
    assert!(!state.commit_pending());
}

// ── set_snapshot replacement ──────────────────────────────────────

#[test]
fn set_snapshot_replaces_previous() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot_with_candidates("ni", SessionStatus::Composing));
    assert_eq!(state.snapshot().unwrap().preedit, "ni");

    state.set_snapshot(snapshot("hao", SessionStatus::Ready));
    let snap = state.snapshot().unwrap();
    assert_eq!(snap.preedit, "hao");
    assert!(
        snap.candidates.is_empty(),
        "candidates from old snapshot must not persist"
    );
}

#[test]
fn set_snapshot_twice_same_status_different_preedit() {
    let mut state = AppState::new();
    state.set_snapshot(snapshot("ni", SessionStatus::Composing));
    assert!(state.has_composition());
    assert!(!state.commit_pending());

    state.set_snapshot(snapshot("", SessionStatus::CommitPending));
    assert!(state.has_composition());
    assert!(state.commit_pending());
    assert_eq!(state.snapshot().unwrap().preedit, "");
}

// ── status setter ─────────────────────────────────────────────────

#[test]
fn set_status_replaces_previous() {
    let mut state = AppState::new();
    assert!(state.status().is_none());

    state.set_status("error: something");
    assert_eq!(state.status(), Some("error: something"));

    state.set_status("ready");
    assert_eq!(state.status(), Some("ready"));
}

// ── should_exit ───────────────────────────────────────────────────

#[test]
fn set_should_exit_flags_exit() {
    let mut state = AppState::new();
    assert!(!state.should_exit());
    state.set_should_exit();
    assert!(state.should_exit());
}
