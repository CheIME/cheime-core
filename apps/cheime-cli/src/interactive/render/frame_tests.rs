use super::frame::build_frame;
use crate::interactive::app::{AppState, LocalAction};
use cheime_model::{
    Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration, Revision, SessionEpoch,
    SessionStatus,
};
use std::path::Path;

fn state_with_snapshot() -> AppState {
    let mut state = AppState::new();
    state.apply_local(LocalAction::Insert('A'));
    state.set_snapshot(CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        deployment: DeploymentGeneration::new(1),
        preedit: String::from("ni"),
        cursor: 2,
        candidates: vec![Candidate::text(CandidateId::new(1), "你", "test")],
        highlighted: Some(CandidateId::new(1)),
        status: SessionStatus::Composing,
        page_size: 9,
        page: 0,
    });
    state
}

#[test]
fn zero_height_is_empty() {
    let frame = build_frame(&AppState::new(), 80, 0, Path::new("demo.log"));

    assert!(frame.lines.is_empty());
    assert!(frame.cursor.is_none());
}

#[test]
fn three_rows_show_editor_candidates_and_status() {
    let frame = build_frame(&state_with_snapshot(), 40, 3, Path::new("logs/demo.log"));

    assert_eq!(frame.lines.len(), 3);
    assert_eq!(frame.lines[0], "Ani");
    assert!(frame.lines[1].contains("[1]你"));
    assert!(frame.lines[2].contains("log:demo.log"));
    assert_eq!(frame.cursor, Some((3, 0)));
}

#[test]
fn one_row_only_shows_status() {
    let frame = build_frame(&state_with_snapshot(), 30, 1, Path::new("demo.log"));

    assert_eq!(frame.lines.len(), 1);
    assert!(frame.lines[0].contains("^C:exit"));
    assert!(frame.cursor.is_none());
}

#[test]
fn status_message_is_rendered() {
    let mut state = AppState::new();
    state.set_status("engine error");

    let frame = build_frame(&state, 40, 2, Path::new("demo.log"));

    assert!(frame.lines[1].contains("engine error"));
}

#[test]
fn narrow_width_truncates_without_invalid_utf8() {
    let frame = build_frame(&state_with_snapshot(), 2, 3, Path::new("logs/demo.log"));

    assert!(
        frame
            .lines
            .iter()
            .all(|line| super::display_width(line) <= 2)
    );
}
