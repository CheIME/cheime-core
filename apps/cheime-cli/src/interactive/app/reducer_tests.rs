use super::*;
use cheime_model::{
    CandidateSnapshot, DeploymentGeneration, Revision, SessionEpoch, SessionStatus,
};

// ── helpers ───────────────────────────────────────────────────────

fn seeded_state(text: &str) -> AppState {
    let mut state = AppState::new();
    state.document.insert(text);
    state
}

fn composing_state() -> AppState {
    let mut state = AppState::new();
    state.set_snapshot(CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        deployment: DeploymentGeneration::new(1),
        preedit: "ni".into(),
        cursor: 2,
        candidates: vec![],
        highlighted: None,
        status: SessionStatus::Composing,
        page_size: 9,
        page: 0,
    });
    state
}

// ── document actions without composition ──────────────────────────

#[test]
fn insert_ascii_appends_and_advances_cursor() {
    let mut state = AppState::new();
    state.apply_local(LocalAction::Insert('a'));
    assert_eq!(state.document().text(), "a");
    assert_eq!(state.document().cursor(), 1);
}

#[test]
fn insert_unicode_scalar_appends_and_advances_cursor_by_utf8_bytes() {
    let mut state = AppState::new();
    state.apply_local(LocalAction::Insert('你'));
    assert_eq!(state.document().text(), "你");
    assert_eq!(state.document().cursor(), 3);
}

#[test]
fn backspace_removes_char_before_cursor() {
    let mut state = seeded_state("ab");
    state.document.move_left(); // cursor at |ab → a|b
    state.apply_local(LocalAction::Backspace);
    assert_eq!(state.document().text(), "b");
    assert_eq!(state.document().cursor(), 0);
}

#[test]
fn delete_removes_char_at_cursor() {
    let mut state = seeded_state("ab");
    state.document.move_home(); // cursor at 0
    state.apply_local(LocalAction::Delete);
    assert_eq!(state.document().text(), "b");
    assert_eq!(state.document().cursor(), 0);
}

#[test]
fn move_left_decrements_cursor_by_scalar() {
    let mut state = seeded_state("ab");
    state.apply_local(LocalAction::MoveLeft);
    assert_eq!(state.document().cursor(), 1);
}

#[test]
fn move_right_increments_cursor_by_scalar() {
    let mut state = seeded_state("ab");
    state.document.move_home(); // cursor at 0
    state.apply_local(LocalAction::MoveRight);
    assert_eq!(state.document().cursor(), 1);
}

#[test]
fn move_home_sets_cursor_to_zero() {
    let mut state = seeded_state("你好");
    state.apply_local(LocalAction::MoveHome);
    assert_eq!(state.document().cursor(), 0);
}

#[test]
fn move_end_sets_cursor_to_text_len() {
    let mut state = seeded_state("你好");
    state.document.move_home();
    state.apply_local(LocalAction::MoveEnd);
    assert_eq!(state.document().cursor(), 6);
}

// ── view / status actions ─────────────────────────────────────────

#[test]
fn toggle_detail_mode_round_trip() {
    let mut state = AppState::new();
    assert!(matches!(state.detail_mode(), DetailMode::Parsed));
    state.apply_local(LocalAction::ToggleDetailMode);
    assert!(matches!(state.detail_mode(), DetailMode::Json));
    state.apply_local(LocalAction::ToggleDetailMode);
    assert!(matches!(state.detail_mode(), DetailMode::Parsed));
}

#[test]
fn toggle_detail_mode_changes_when_composition_is_active() {
    let mut state = composing_state();

    state.apply_local(LocalAction::ToggleDetailMode);

    assert!(matches!(state.detail_mode(), DetailMode::Json));
}

#[test]
fn scroll_up_at_zero_stays_zero() {
    let mut state = AppState::new();
    assert_eq!(state.detail_scroll(), 0);
    state.apply_local(LocalAction::ScrollUp);
    assert_eq!(state.detail_scroll(), 0);
}

#[test]
fn scroll_down_increments() {
    let mut state = AppState::new();
    state.apply_local(LocalAction::ScrollDown);
    assert_eq!(state.detail_scroll(), 1);
    state.apply_local(LocalAction::ScrollDown);
    assert_eq!(state.detail_scroll(), 2);
}

#[test]
fn scroll_up_decrements_from_nonzero() {
    let mut state = AppState::new();
    state.detail_scroll = 3;
    state.apply_local(LocalAction::ScrollUp);
    assert_eq!(state.detail_scroll(), 2);
}

#[test]
fn scroll_up_decrements_when_composition_is_active() {
    let mut state = composing_state();
    state.detail_scroll = 1;

    state.apply_local(LocalAction::ScrollUp);

    assert_eq!(state.detail_scroll(), 0);
}

#[test]
fn scroll_down_increments_when_composition_is_active() {
    let mut state = composing_state();

    state.apply_local(LocalAction::ScrollDown);

    assert_eq!(state.detail_scroll(), 1);
}

#[test]
fn scroll_down_saturates_at_max() {
    let mut state = AppState::new();
    state.detail_scroll = usize::MAX;
    state.apply_local(LocalAction::ScrollDown);
    assert_eq!(state.detail_scroll(), usize::MAX);
}

#[test]
fn clear_status_from_some_sets_to_none() {
    let mut state = AppState::new();
    state.set_status("an error");
    assert!(state.status().is_some());
    state.apply_local(LocalAction::ClearStatus);
    assert!(state.status().is_none());
}

#[test]
fn clear_status_clears_some_when_composition_is_active() {
    let mut state = composing_state();
    state.set_status("an error");

    state.apply_local(LocalAction::ClearStatus);

    assert!(state.status().is_none());
}

#[test]
fn clear_status_when_already_none_stays_none() {
    let mut state = AppState::new();
    assert!(state.status().is_none());
    state.apply_local(LocalAction::ClearStatus);
    assert!(state.status().is_none());
}
