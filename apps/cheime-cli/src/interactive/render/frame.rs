use super::super::app::AppState;
use super::{display_width, truncate_columns};
use std::path::Path;

pub(crate) struct Frame {
    pub(super) lines: Vec<String>,
    pub(super) cursor: Option<(u16, u16)>,
}

pub(crate) fn build_frame(state: &AppState, width: u16, height: u16, log_path: &Path) -> Frame {
    let width = width as usize;
    let height = height as usize;
    if height == 0 {
        return Frame {
            lines: Vec::new(),
            cursor: None,
        };
    }

    let status_row = height.saturating_sub(1);
    let mut lines = Vec::with_capacity(height);
    let mut cursor = None;

    if height >= 2 {
        let (line, column) = build_editor_row(state, width);
        lines.push(line);
        cursor = Some((column.min(u16::MAX as usize) as u16, 0));
    }
    if height >= 3 {
        lines.push(build_candidate_row(state, width));
    }
    while lines.len() < status_row {
        lines.push(String::new());
    }
    lines.push(build_status_row(state, width, log_path));

    Frame { lines, cursor }
}

fn build_editor_row(state: &AppState, width: usize) -> (String, usize) {
    let document = state.document();
    let before = &document.text()[..document.cursor()];
    let after = &document.text()[document.cursor()..];
    let before_width = display_width(before);

    match state.snapshot() {
        Some(snapshot) if !snapshot.preedit.is_empty() => {
            let preedit_before = &snapshot.preedit[..snapshot.cursor];
            let line = format!("{before}{}{after}", snapshot.preedit);
            (
                truncate_columns(&line, width),
                (before_width + display_width(preedit_before)).min(width),
            )
        }
        _ => (
            truncate_columns(document.text(), width),
            before_width.min(width),
        ),
    }
}

fn build_candidate_row(state: &AppState, width: usize) -> String {
    let Some(snapshot) = state.snapshot() else {
        return String::new();
    };
    let mut parts = Vec::with_capacity(snapshot.candidates.len().min(9) + 1);
    for (index, candidate) in snapshot.candidates.iter().enumerate().take(9) {
        let number = index + 1;
        let prefix = if Some(candidate.id) == snapshot.highlighted {
            format!("[{number}]")
        } else {
            format!("{number}.")
        };
        parts.push(format!("{prefix}{} ", candidate.text));
    }
    parts.push(format!("(Pg {})", snapshot.page + 1));
    truncate_columns(&parts.join(""), width)
}

fn build_status_row(state: &AppState, width: usize, log_path: &Path) -> String {
    let log_name = log_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("-");
    let mut line = format!("log:{log_name}  ^C:exit");
    if let Some(status) = state.status() {
        line.push_str("  ");
        line.push_str(status);
    }
    pad_to_width(&line, width)
}

fn pad_to_width(text: &str, width: usize) -> String {
    let current = display_width(text);
    if current >= width {
        return truncate_columns(text, width);
    }
    format!("{text}{}", " ".repeat(width - current))
}
