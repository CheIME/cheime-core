//! Terminal renderer — writes a `Frame` to a `Write` sink via crossterm.
//!
//! Pure I/O: no cursor-hide, no alternate-screen, no raw-mode.  Those belong
//! to the terminal setup layer (Task 7).

use super::frame::Frame;
use crossterm::{cursor::MoveTo, style::Print, terminal::Clear, terminal::ClearType, QueueableCommand};
use std::io::{self, Write};

/// Renders `frame` to `writer` via batched crossterm commands.
///
/// Each line is positioned at `(0, row)`, printed, and cleared to end-of-line.
/// After all lines, the cursor moves to the frame-specified position (if any),
/// otherwise to `(0, 0)`.
pub(super) fn render_frame<W: Write>(writer: &mut W, frame: &Frame) -> io::Result<()> {
    for (row, line) in frame.lines.iter().enumerate() {
        writer.queue(MoveTo(0, row as u16))?;
        writer.queue(Print(line.as_str()))?;
        writer.queue(Clear(ClearType::UntilNewLine))?;
    }

    if let Some((col, row)) = frame.cursor {
        writer.queue(MoveTo(col, row))?;
    } else {
        writer.queue(MoveTo(0, 0))?;
    }

    writer.flush()
}
