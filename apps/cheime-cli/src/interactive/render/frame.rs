//! Pure Frame layout builder for the interactive TUI.
//!
//! Builds a `Frame` (lines + terminal cursor position) from `AppState`,
//! terminal width, terminal height, and an optional log path.
//! No ANSI, no crossterm, no terminal I/O — pure data.

use super::super::app::{AppState, DetailMode};
use super::{display_width, truncate_columns};
use std::path::Path;

// ── Frame ─────────────────────────────────────────────────────────────────

pub(crate) struct Frame {
    pub(super) lines: Vec<String>,
    pub(super) cursor: Option<(u16, u16)>,
}

// ── entry point ───────────────────────────────────────────────────────────

pub(crate) fn build_frame(
    state: &AppState,
    width: u16,
    height: u16,
    log_path: Option<&Path>,
) -> Frame {
    let width_usize = width as usize;
    let height_usize = height as usize;

    if height_usize == 0 {
        return Frame {
            lines: Vec::new(),
            cursor: None,
        };
    }

    let snapshot = state.snapshot();
    let status_row_index = height_usize.saturating_sub(1);
    let mut lines: Vec<String> = Vec::with_capacity(height_usize);
    let mut cursor: Option<(u16, u16)> = None;

    // ── editor row (row 0, only when height >= 2) ───────────────────────
    if height_usize >= 2 {
        let (line, col) = build_editor_row(state, snapshot, width_usize);
        lines.push(line);
        cursor = Some((col.clamp(0, u16::MAX as usize) as u16, 0));
    }

    // ── candidate row (row 1, only when height >= 3) ────────────────────
    if height_usize >= 3 {
        lines.push(build_candidate_row(snapshot, width_usize));

        // Detail rows fill the gap between candidates and status.
        let detail_start = lines.len();
        let detail_slot_count = status_row_index.saturating_sub(detail_start);
        if detail_slot_count > 0 {
            let detail_lines = build_detail_lines(state, snapshot, width_usize, detail_slot_count);
            lines.extend(detail_lines);
        }
    }

    // Pad empty lines so that the status row lands at `status_row_index`.
    while lines.len() < status_row_index {
        lines.push(String::new());
    }

    // ── status row (always last) ────────────────────────────────────────
    lines.push(build_status_row(state, width_usize, log_path));

    // height == 1 implies no editor row → no cursor.
    if height_usize == 1 {
        cursor = None;
    }

    Frame { lines, cursor }
}

// ── editor row ────────────────────────────────────────────────────────────

/// Returns `(line_text, cursor_column)`.
fn build_editor_row(
    state: &AppState,
    snapshot: Option<&cheime_model::CandidateSnapshot>,
    width: usize,
) -> (String, usize) {
    let doc_cursor = state.document().cursor();
    let doc_text = state.document().text();

    let before_doc = &doc_text[..doc_cursor];
    let after_doc = &doc_text[doc_cursor..];
    let before_width = display_width(before_doc);

    match snapshot {
        Some(snap) if !snap.preedit.is_empty() => {
            let preedit_before = &snap.preedit[..snap.cursor];
            let preedit_after = &snap.preedit[snap.cursor..];

            let preedit_before_width = display_width(preedit_before);

            let line = format!(
                "{}{}{}{}",
                before_doc, preedit_before, preedit_after, after_doc
            );
            let cursor_col = (before_width + preedit_before_width).min(width);
            (truncate_columns(&line, width), cursor_col)
        }
        _ => {
            // No composition: show document text, cursor at doc position.
            let line = format!("{}{}", before_doc, after_doc);
            let cursor_col = before_width.min(width);
            (truncate_columns(&line, width), cursor_col)
        }
    }
}

// ── candidate row ─────────────────────────────────────────────────────────

fn build_candidate_row(snapshot: Option<&cheime_model::CandidateSnapshot>, width: usize) -> String {
    let snap = match snapshot {
        Some(s) => s,
        None => return String::new(),
    };

    let mut parts: Vec<String> = Vec::with_capacity(snap.candidates.len().min(9) + 1);

    for (i, candidate) in snap.candidates.iter().enumerate().take(9) {
        let num = i + 1;
        let highlighted = Some(candidate.id) == snap.highlighted;

        let prefix = if highlighted {
            format!("[{}]", num)
        } else {
            format!("{}.", num)
        };
        parts.push(format!("{}{} ", prefix, candidate.text));
    }

    parts.push(format!("(Pg {})", snap.page + 1));

    let line = parts.join("");
    truncate_columns(&line, width)
}

// ── detail region ─────────────────────────────────────────────────────────

fn build_detail_lines(
    state: &AppState,
    snapshot: Option<&cheime_model::CandidateSnapshot>,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    match state.detail_mode() {
        DetailMode::Parsed => {
            build_parsed_details(snapshot, width, max_rows, state.detail_scroll())
        }
        DetailMode::Json => build_json_details(snapshot, width, max_rows, state.detail_scroll()),
    }
}

fn build_parsed_details(
    snapshot: Option<&cheime_model::CandidateSnapshot>,
    width: usize,
    max_rows: usize,
    scroll: usize,
) -> Vec<String> {
    let snap = match snapshot {
        Some(s) => s,
        None => return vec![truncate_columns("[No snapshot]", width)],
    };

    let raw_lines: Vec<String> = vec![
        format!(
            "Revision: {:?}  Epoch: {:?}  Deployment: {:?}",
            snap.revision, snap.epoch, snap.deployment
        ),
        format!(
            "Preedit: \"{}\"  Cursor: {}  Status: {:?}",
            snap.preedit, snap.cursor, snap.status
        ),
        format!(
            "Candidates: {}  highlighted: {:?}  Page: {}/{}",
            snap.candidates.len(),
            snap.highlighted,
            snap.page + 1,
            snap.page_size,
        ),
    ];

    raw_lines
        .into_iter()
        .skip(scroll)
        .take(max_rows)
        .map(|l| truncate_columns(&l, width))
        .collect()
}

fn build_json_details(
    snapshot: Option<&cheime_model::CandidateSnapshot>,
    width: usize,
    max_rows: usize,
    scroll: usize,
) -> Vec<String> {
    let snap = match snapshot {
        Some(s) => s,
        None => return vec![truncate_columns("[No snapshot]", width)],
    };

    let json_text = match serde_json::to_string_pretty(snap) {
        Ok(json) => json,
        Err(e) => format!("{{ \"serialization_error\": \"{e}\" }}"),
    };

    json_text
        .lines()
        .skip(scroll)
        .take(max_rows)
        .map(|l| truncate_columns(l, width))
        .collect()
}

// ── status row ────────────────────────────────────────────────────────────

fn build_status_row(state: &AppState, width: usize, log_path: Option<&Path>) -> String {
    let mode_name = match state.detail_mode() {
        DetailMode::Parsed => "Parsed",
        DetailMode::Json => "Json",
    };

    let log_snippet = log_path
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("-");

    let mut parts = vec![
        format!("[{mode_name}]"),
        format!("log:{log_snippet}"),
        "F2:detail ^C:exit".to_owned(),
    ];

    if let Some(status_msg) = state.status() {
        parts.push(status_msg.to_owned());
    }

    let line = parts.join("  ");
    pad_to_width(&line, width)
}

// ── helpers ───────────────────────────────────────────────────────────────

/// Right-pad `text` with spaces so its display width equals `target_width`.
/// If `text` already exceeds `target_width`, truncate to fit.
fn pad_to_width(text: &str, target_width: usize) -> String {
    let dw = display_width(text);
    if dw >= target_width {
        return truncate_columns(text, target_width);
    }
    let padding = " ".repeat(target_width - dw);
    format!("{text}{padding}")
}
