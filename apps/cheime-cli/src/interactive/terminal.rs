//! Terminal lifecycle: raw mode, alternate screen, cursor visibility, and event
//! polling.  The `Terminal` struct acts as an RAII guard — restoring the original
//! terminal state on `Drop`.

use crossterm::{
    cursor,
    event::{self, Event, KeyEvent},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Write};
use std::time::Duration;

/// Manages terminal state for the duration of the interactive session.
///
/// Construction enters raw mode, the alternate screen, and hides the cursor.
/// `Drop` reverses these in order: show cursor, leave alternate screen,
/// disable raw mode.
pub(super) struct Terminal {
    /// Tracks whether we successfully entered the alternate screen (only
    /// leave it when we entered it).
    alternate_screen_entered: bool,
}

impl Terminal {
    /// Enters raw mode, alternate screen, and hides the cursor.
    ///
    /// On error the caller should *not* drop the `Terminal` without
    /// manually restoring whatever was partially enabled.
    pub(super) fn init() -> io::Result<Self> {
        terminal::enable_raw_mode()?;

        // Enter alternate screen — if this fails, we still want raw mode
        // restored, which the Drop guard handles.
        let mut stdout = io::stdout();
        let alternate_ok = execute!(stdout, EnterAlternateScreen).is_ok();

        // Hide cursor — best-effort.
        let _ = execute!(stdout, cursor::Hide);

        Ok(Self {
            alternate_screen_entered: alternate_ok,
        })
    }

    /// Blocking read of the next keyboard event.
    ///
    /// Non-key events (focus, mouse, resize) are silently skipped; only
    /// `KeyEvent` values are returned.  This keeps the caller simple: every
    /// frame tick processes exactly one key.
    pub(super) fn read_key(&self) -> io::Result<KeyEvent> {
        loop {
            match event::read()? {
                Event::Key(key) => return Ok(key),
                Event::Resize(..) => {
                    // The render loop picks up new dimensions on the next
                    // frame; no action needed here.
                }
                _ => {
                    // Mouse, focus, paste — ignored.
                }
            }
        }
    }

    /// Non-blocking poll for a key event.
    ///
    /// Returns `Ok(None)` when no key is available within `timeout`.
    /// Like `read_key`, non-key events are filtered out.
    pub(super) fn try_read_key(&self, timeout: Duration) -> io::Result<Option<KeyEvent>> {
        if !event::poll(timeout)? {
            return Ok(None);
        }
        match event::read()? {
            Event::Key(key) => Ok(Some(key)),
            _ => {
                // We polled, got a non-key event — consume it and return
                // None so the caller can try again next frame.
                Ok(None)
            }
        }
    }

    /// Returns the current terminal size as `(cols, rows)`.
    pub(super) fn size() -> io::Result<(u16, u16)> {
        terminal::size()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let mut stdout = io::stdout();

        // Show cursor (best-effort).
        let _ = execute!(stdout, cursor::Show);

        if self.alternate_screen_entered {
            let _ = execute!(stdout, LeaveAlternateScreen);
        }

        let _ = terminal::disable_raw_mode();
        // Flush whatever crossterm queued during restoration.
        let _ = stdout.flush();
    }
}
