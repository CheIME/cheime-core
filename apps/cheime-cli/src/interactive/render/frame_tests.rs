use super::super::app::AppState;
use super::display_width;
use super::frame::build_frame;
use cheime_model::{
    Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration, Revision, SessionEpoch,
    SessionStatus,
};
use std::path::Path;

// ── helpers ──────────────────────────────────────────────────────────────

fn make_candidate(id: u64, text: &str) -> Candidate {
    Candidate {
        id: CandidateId::new(id),
        text: text.to_owned(),
        annotation: None,
        source: "test".to_owned(),
        is_emoji: false,
    }
}

fn make_snapshot(
    preedit: &str,
    cursor: usize,
    candidates: Vec<Candidate>,
    highlighted: Option<u64>,
) -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(42),
        deployment: DeploymentGeneration::new(1),
        preedit: preedit.to_owned(),
        cursor,
        candidates,
        highlighted: highlighted.map(CandidateId::new),
        status: SessionStatus::Composing,
        page_size: 9,
        page: 0,
    }
}

// ── height-0 ─────────────────────────────────────────────────────────────

#[test]
fn build_frame_height_zero_returns_empty_frame() {
    // Given
    let state = AppState::new();

    // When
    let frame = build_frame(&state, 80, 0, None);

    // Then
    assert!(frame.lines.is_empty(), "expected empty lines for height 0");
    assert_eq!(frame.cursor, None, "expected no cursor for height 0");
}

// ── height-1: status only ────────────────────────────────────────────────

#[test]
fn build_frame_height_one_shows_status_only_without_cursor() {
    // Given
    let mut state = AppState::new();
    state.set_status("hello");

    // When
    let frame = build_frame(&state, 80, 1, None);

    // Then
    assert_eq!(frame.lines.len(), 1, "single status line");
    assert!(
        frame.lines[0].contains("hello"),
        "status line carries status message"
    );
    assert_eq!(frame.cursor, None, "no cursor for height 1 (no editor row)");
}

// ── height-2: editor + status ────────────────────────────────────────────

#[test]
fn build_frame_height_two_shows_editor_and_status() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::Insert('h'));
    state.apply_local(super::super::app::LocalAction::Insert('i'));

    // When
    let frame = build_frame(&state, 80, 2, Some(Path::new("/tmp/test.log")));

    // Then
    assert_eq!(frame.lines.len(), 2);
    assert!(frame.lines[0].contains("hi"), "editor row shows document");
    assert!(
        frame.lines[1].contains("test.log"),
        "status row shows log name"
    );
    assert!(frame.cursor.is_some(), "cursor on editor row");
    let (col, row) = frame.cursor.unwrap();
    assert_eq!(row, 0, "cursor row is 0 (editor)");
    assert!(col > 0, "cursor column after 'hi'");
}

#[test]
fn build_frame_cursor_position_for_ascii_document_no_composition() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::Insert('a'));
    state.apply_local(super::super::app::LocalAction::Insert('b'));
    state.apply_local(super::super::app::LocalAction::Insert('c'));
    // cursor at end: "abc"

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    let (col, _row) = frame.cursor.unwrap();
    assert_eq!(col, 3, "cursor at display column 3 (after 'abc')");
}

#[test]
fn build_frame_cursor_position_for_cjk_document_no_composition() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::Insert('你'));
    state.apply_local(super::super::app::LocalAction::Insert('好'));
    // cursor at end: "你好" (display width 4)

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    let (col, _row) = frame.cursor.unwrap();
    assert_eq!(col, 4, "cursor at display column 4 after CJK '你好'");
}

// ── height-3+: full layout ───────────────────────────────────────────────

#[test]
fn build_frame_height_three_shows_editor_candidates_status() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::Insert('n'));
    state.apply_local(super::super::app::LocalAction::Insert('i'));

    let snap = make_snapshot(
        "ni",
        2,
        vec![
            make_candidate(1, "你"),
            make_candidate(2, "拟"),
            make_candidate(3, "尼"),
        ],
        Some(1),
    );
    state.set_snapshot(snap);

    // When
    let frame = build_frame(&state, 80, 3, None);

    // Then
    assert_eq!(frame.lines.len(), 3);
    // editor: "nini" (document "ni" + preedit "ni" inlined at end — wait, document is "ni" and preedit is "ni" at cursor (end of document))
    // Actually document: "ni" (cursor at end, byte 2), preedit: "ni"
    // So line = "ni" + "ni" = "nini"
    assert!(
        frame.lines[0].contains("nini"),
        "editor shows document+preedit"
    );
    assert!(
        frame.lines[1].contains("你"),
        "candidate row shows candidates"
    );
    assert!(frame.lines[1].contains("1"), "candidate numbering");
}

#[test]
fn build_frame_cursor_in_preedit_for_ascii() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::Insert('h'));
    // document: "h", cursor at byte 1 (end)

    let snap = make_snapshot("ello", 0, vec![], None);
    state.set_snapshot(snap);
    // preedit "ello" inserted at cursor: "hello"
    // preedit cursor at 0, so logical cursor at display_width("h") + 0 = 1

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    let (col, row) = frame.cursor.unwrap();
    assert_eq!(row, 0);
    assert_eq!(
        col, 1,
        "cursor at display col 1 (after 'h', start of preedit)"
    );
}

#[test]
fn build_frame_cursor_in_preedit_for_cjk() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::Insert('你'));
    // document: "你", cursor at byte 3 (end), display width 2

    let snap = make_snapshot("hao", 1, vec![], None);
    // preedit "hao" at cursor, "h"=1 byte. preedit_cursor byte 1.
    // display_width("你") = 2, display_width("h") = 1, cursor_col = 3
    state.set_snapshot(snap);

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    let (col, row) = frame.cursor.unwrap();
    assert_eq!(row, 0);
    assert_eq!(col, 3, "cursor after CJK 你(2w) + preedit h(1w) = 3");
}

#[test]
fn build_frame_candidate_highlight_shows_bracket_indicator() {
    // Given
    let mut state = AppState::new();
    let snap = make_snapshot(
        "ni",
        2,
        vec![make_candidate(1, "你"), make_candidate(2, "拟")],
        Some(2), // highlighted is candidate 2
    );
    state.set_snapshot(snap);

    // When
    let frame = build_frame(&state, 80, 3, None);

    // Then
    let cand_line = &frame.lines[1];
    assert!(cand_line.contains("[2]"), "highlighted candidate shows [2]");
}

#[test]
fn build_frame_candidate_page_info_shown() {
    // Given
    let mut state = AppState::new();
    let snap = make_snapshot("ni", 2, vec![make_candidate(1, "你")], Some(1));
    state.set_snapshot(snap);

    // When
    let frame = build_frame(&state, 80, 3, None);

    // Then
    assert!(frame.lines[1].contains("Pg"), "candidate row has page info");
}

#[test]
fn build_frame_parsed_detail_mode_shows_metadata() {
    // Given
    let mut state = AppState::new();
    let snap = make_snapshot(
        "ni",
        2,
        vec![make_candidate(1, "你"), make_candidate(2, "拟")],
        Some(1),
    );
    state.set_snapshot(snap);
    // default mode is Parsed

    // When
    let frame = build_frame(&state, 80, 5, None);

    // Then
    let detail_text: String = frame.lines[2..4]
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        detail_text.contains("Revision"),
        "parsed detail shows revision"
    );
    assert!(detail_text.contains("ni"), "parsed detail shows preedit");
}

#[test]
fn build_frame_json_detail_mode_shows_pretty_json() {
    // Given
    let mut state = AppState::new();
    let snap = make_snapshot("ni", 2, vec![make_candidate(1, "你")], Some(1));
    state.set_snapshot(snap);
    state.apply_local(super::super::app::LocalAction::ToggleDetailMode);
    // now Json mode

    // When: height = 15 gives 13 detail rows (enough for pretty JSON)
    let frame = build_frame(&state, 80, 15, None);

    // Then
    let detail_text: String = frame.lines[2..14]
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(detail_text.contains('{'), "json detail has opening brace");
    assert!(
        detail_text.contains("\"preedit\""),
        "json detail contains preedit field"
    );
}

#[test]
fn build_frame_status_row_shows_detail_mode_name() {
    // Given
    let state = AppState::new();

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    assert!(
        frame.lines[1].contains("Parsed"),
        "status shows 'Parsed' by default"
    );
}

#[test]
fn build_frame_status_row_shows_json_after_toggle() {
    // Given
    let mut state = AppState::new();
    state.apply_local(super::super::app::LocalAction::ToggleDetailMode);

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    assert!(
        frame.lines[1].contains("Json"),
        "status shows 'Json' after toggle"
    );
}

#[test]
fn build_frame_status_row_shows_log_path() {
    // Given
    let state = AppState::new();
    let log_path = Path::new("/var/log/cheime/2026-07-23.jsonl");

    // When
    let frame = build_frame(&state, 80, 2, Some(log_path));

    // Then
    assert!(
        frame.lines[1].contains("2026-07-23"),
        "status shows log filename"
    );
}

#[test]
fn build_frame_status_row_shows_hints() {
    // Given
    let state = AppState::new();

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    assert!(frame.lines[1].contains("F2"), "status shows F2 hint");
}

#[test]
fn build_frame_status_message_propagates() {
    // Given
    let mut state = AppState::new();
    state.set_status("disk full");

    // When
    let frame = build_frame(&state, 80, 2, None);

    // Then
    assert!(
        frame.lines[1].contains("disk full"),
        "status line carries status message"
    );
}

#[test]
fn build_frame_width_truncates_long_candidate_row() {
    // Given
    let mut state = AppState::new();
    let snap = make_snapshot(
        "ni",
        2,
        vec![
            make_candidate(1, "你好世界很长"),
            make_candidate(2, "你好"),
            make_candidate(3, "拟"),
        ],
        Some(1),
    );
    state.set_snapshot(snap);

    // When
    let frame = build_frame(&state, 10, 3, None);

    // Then
    let cand_line = &frame.lines[1];
    assert!(
        display_width(cand_line) <= 10,
        "candidate row truncated to width"
    );
    assert!(
        cand_line.contains("你好"),
        "truncation preserves partial CJK"
    );
}

#[test]
fn build_frame_empty_snapshot_handled_gracefully() {
    // Given
    let state = AppState::new();
    // no snapshot set

    // When (should not panic)
    let frame = build_frame(&state, 80, 5, None);

    // Then
    assert!(!frame.lines.is_empty());
    // no candidates, no preedit, just editor (with block cursor for empty doc) + detail lines + status
}

#[test]
fn build_frame_very_narrow_terminal_does_not_panic() {
    // Given
    let mut state = AppState::new();
    state.set_status("test");
    let snap = make_snapshot("ni", 1, vec![make_candidate(1, "你")], Some(1));
    state.set_snapshot(snap);

    // When (width = 1)
    let frame = build_frame(&state, 1, 5, None);

    // Then
    for line in &frame.lines {
        assert!(display_width(line) <= 1, "no line exceeds width 1");
    }
}

#[test]
fn build_frame_status_row_padded_to_full_width() {
    // Given
    let state = AppState::new();

    // When
    let frame = build_frame(&state, 40, 2, None);

    // Then
    assert_eq!(
        display_width(&frame.lines[1]),
        40,
        "status row padded to full width"
    );
}
