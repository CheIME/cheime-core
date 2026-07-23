use super::frame::Frame;
use super::writer::render_frame;

// ── helpers ─────────────────────────────────────────────────────────────────

fn frame(lines: Vec<&str>) -> Frame {
    Frame {
        lines: lines.into_iter().map(String::from).collect(),
        cursor: None,
    }
}

fn frame_with_cursor(lines: Vec<&str>, col: u16, row: u16) -> Frame {
    Frame {
        lines: lines.into_iter().map(String::from).collect(),
        cursor: Some((col, row)),
    }
}

/// ANSI CSI prefix used by all crossterm cursor/clear commands.
const CSI: &str = "\x1b[";

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected output to contain {needle:?}, got:\n{haystack:?}"
    );
}

// ── single line ─────────────────────────────────────────────────────────────

#[test]
fn render_single_line_writes_at_row_zero() {
    let f = frame(vec!["hello"]);
    let mut buf = Vec::new();
    render_frame(&mut buf, &f).unwrap();
    let out = String::from_utf8(buf).unwrap();

    assert_contains(&out, "hello");
    // MoveTo(0,0): ESC [ 1 ; 1 H (rows are 1-based, cols are 1-based)
    assert_contains(&out, &format!("{CSI}1;1H"));
    // ClearUntilNewLine: ESC [ K  (or ESC [ 0 K)
    assert_contains(&out, &format!("{CSI}K"));
}

// ── multiple lines ──────────────────────────────────────────────────────────

#[test]
fn render_two_lines_writes_each_at_correct_row() {
    let f = frame(vec!["alpha", "beta"]);
    let mut buf = Vec::new();
    render_frame(&mut buf, &f).unwrap();
    let out = String::from_utf8(buf).unwrap();

    assert_contains(&out, "alpha");
    assert_contains(&out, "beta");
    // row 0 → (1,1) H
    assert_contains(&out, &format!("{CSI}1;1H"));
    // row 1 → (2,1) H
    assert_contains(&out, &format!("{CSI}2;1H"));
}

// ── cursor positioning ──────────────────────────────────────────────────────

#[test]
fn render_with_cursor_moves_to_frame_cursor() {
    let f = frame_with_cursor(vec!["text"], 7, 2);
    let mut buf = Vec::new();
    render_frame(&mut buf, &f).unwrap();
    let out = String::from_utf8(buf).unwrap();

    // After the line, move to (col 7, row 2) → (3, 8) H
    assert_contains(&out, &format!("{CSI}3;8H"));
}

#[test]
fn render_without_cursor_defaults_to_origin() {
    let f = frame(vec!["text"]);
    let mut buf = Vec::new();
    render_frame(&mut buf, &f).unwrap();
    let out = String::from_utf8(buf).unwrap();

    // Last MoveTo should be to (0,0) → (1,1) H
    let last_move = out.rfind(&format!("{CSI}1;1H")).unwrap();
    // There should be at least two: one for line 0, one for final cursor
    let first_move = out.find(&format!("{CSI}1;1H")).unwrap();
    assert_ne!(first_move, last_move, "expected two MoveTo(0,0) calls");
}

// ── empty frame ─────────────────────────────────────────────────────────────

#[test]
fn render_empty_frame_only_positions_cursor() {
    let f = frame(vec![]);
    let mut buf = Vec::new();
    render_frame(&mut buf, &f).unwrap();
    let out = String::from_utf8(buf).unwrap();

    // Only a MoveTo(0,0) and flush — no line content
    assert_contains(&out, &format!("{CSI}1;1H"));
    assert!(!out.contains("hello"));
}

// ── flush behavior ──────────────────────────────────────────────────────────

/// A writer that panics if flush is not called before drop.
struct FlushRequired {
    inner: Vec<u8>,
    flushed: bool,
}

impl std::io::Write for FlushRequired {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushed = true;
        self.inner.flush()
    }
}

impl Drop for FlushRequired {
    fn drop(&mut self) {
        assert!(self.flushed, "render_frame must call flush before returning");
    }
}

#[test]
fn render_flushes_before_returning() {
    let f = frame(vec!["x"]);
    let mut writer = FlushRequired {
        inner: Vec::new(),
        flushed: false,
    };
    render_frame(&mut writer, &f).unwrap();
    // The Drop assertion ensures flush was called.
}

// ── clear-after-print ───────────────────────────────────────────────────────

#[test]
fn render_clears_to_end_of_line_after_each_line() {
    let f = frame(vec!["a", "b", "c"]);
    let mut buf = Vec::new();
    render_frame(&mut buf, &f).unwrap();
    let out = String::from_utf8(buf).unwrap();

    // Each line should have a Clear(UntilNewLine) — CSI K
    // Three lines → three clears
    let clears = out.matches(&format!("{CSI}K")).count();
    assert_eq!(clears, 3, "expected 3 Clear(UntilNewLine) for 3 lines");
}
