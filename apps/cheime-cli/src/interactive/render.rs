use unicode_width::UnicodeWidthStr;

/// Returns the terminal display width defined by `unicode-width`.
pub(super) fn display_width(text: &str) -> usize {
    text.width()
}

/// Returns the longest scalar-aligned prefix that fits within `max_columns`.
pub(super) fn truncate_columns(text: &str, max_columns: usize) -> String {
    let mut end = 0;

    for (start, character) in text.char_indices() {
        let next = start + character.len_utf8();
        if text[..next].width() <= max_columns {
            end = next;
        }
    }

    text[..end].to_owned()
}

pub(super) mod frame;
pub(super) mod writer;

#[cfg(test)]
mod frame_tests;

#[cfg(test)]
mod render_tests;

#[cfg(test)]
mod writer_tests;
