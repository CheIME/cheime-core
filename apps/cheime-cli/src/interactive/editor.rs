//! CLI-local Unicode document buffer.
//!
//! `Document` owns a UTF-8 text buffer and a byte-offset cursor that is always
//! at a character boundary.  Insertion appends at the cursor position and
//! advances the cursor by the inserted byte count.

pub(super) struct Document {
    text: String,
    cursor: usize,
}

impl Document {
    /// Empty document with cursor at position 0.
    pub(super) fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    /// The full document text.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// Current cursor position as a UTF-8 byte offset.
    pub(super) fn cursor(&self) -> usize {
        self.cursor
    }

    /// Insert `text` at the cursor position and advance the cursor.
    ///
    /// # Panics
    ///
    /// Panics if `cursor` is not a valid UTF-8 character boundary or is out of
    /// bounds (delegated to [`String::insert_str`]).
    pub(super) fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// Move cursor left by one Unicode scalar if not already at position 0.
    pub(super) fn move_left(&mut self) {
        if self.cursor > 0 {
            if let Some((prev, _)) = self.text[..self.cursor].char_indices().next_back() {
                self.cursor = prev;
            }
        }
    }

    /// Move cursor right by one Unicode scalar if not already at end.
    pub(super) fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            if let Some(ch) = self.text[self.cursor..].chars().next() {
                self.cursor += ch.len_utf8();
            }
        }
    }

    /// Move cursor to the beginning of the document.
    pub(super) fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the document.
    pub(super) fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Delete the Unicode scalar immediately before the cursor.
    ///
    /// Returns `true` when a scalar was removed, `false` when the cursor is
    /// at position 0 (no-op).  Moves the cursor to the start of the deleted
    /// scalar.
    pub(super) fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        if let Some((prev, _ch)) = self.text[..self.cursor].char_indices().next_back() {
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
            true
        } else {
            false
        }
    }

    /// Delete the Unicode scalar at the cursor.
    ///
    /// Returns `true` when a scalar was removed, `false` when the cursor is at
    /// the end of the document (no-op).  Does not move the cursor.
    pub(super) fn delete(&mut self) -> bool {
        if self.cursor == self.text.len() {
            return false;
        }
        if let Some(ch) = self.text[self.cursor..].chars().next() {
            let end = self.cursor + ch.len_utf8();
            self.text.replace_range(self.cursor..end, "");
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod deletion_tests;
#[cfg(test)]
mod editor_tests;
