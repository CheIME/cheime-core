//! Key mapper: translates physical key events into logical input characters.
//!
//! This is the first stage of the unified input model (DRAFT §6).
//! Different input schemes replace this component:
//!
//! | Scheme      | KeyMapper         |
//! |-------------|-------------------|
//! | QuanPin     | Passthrough (a-z) |
//! | Flypy (小鹤)| 2-key → pinyin    |
//! | MSPY (微软) | 2-key → pinyin    |
//! | Wubi        | Multi-key → shape |

use cheime_model::{Key, KeyEvent};

/// Result of key mapping.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyMapResult {
    /// The logical character produced by the mapper.
    pub character: Option<char>,
    /// Whether the key was consumed (don't pass to further processing).
    pub consumed: bool,
}

/// Maps physical key events to logical input characters.
pub trait KeyMapper: Send + Sync {
    fn map(&self, event: &KeyEvent) -> KeyMapResult;
}

// ── QuanPin (全拼) ─────────────────────────────────────────────────

/// Passthrough mapper — ASCII lowercase letters pass through unchanged.
#[derive(Clone, Debug, Default)]
pub struct QuanPinMapper;

impl KeyMapper for QuanPinMapper {
    fn map(&self, event: &KeyEvent) -> KeyMapResult {
        match event.key {
            Key::Character(ch) if ch.is_ascii_lowercase() => KeyMapResult {
                character: Some(ch),
                consumed: false,
            },
            _ => KeyMapResult::default(),
        }
    }
}

// ── Flypy (小鹤双拼) ───────────────────────────────────────────────

/// Xiaohe double-pinyin (小鹤双拼) key mapping.
///
/// Each consonant key maps to its pinyin initial. Each vowel key maps
/// to a complete pinyin final. Two keystrokes = one syllable.
///
/// Reference: https://github.com/rime/rime-double-pinyin (flypy layout)
#[derive(Clone, Debug, Default)]
pub struct FlypyMapper {
    /// Buffered initial character, waiting for final key.
    buffer: Option<char>,
}

impl FlypyMapper {
    pub fn new() -> Self {
        Self { buffer: None }
    }

    /// Map a consonant key to its pinyin initial.
    fn map_initial(ch: char) -> Option<char> {
        match ch {
            'b' | 'p' | 'm' | 'f' | 'd' | 't' | 'n' | 'l' | 'g' | 'k' | 'h' | 'j'
            | 'q' | 'x' | 'r' | 'z' | 'c' | 's' | 'y' | 'w' => Some(ch),
            // Flypy special: v→zh, i→ch, u→sh
            'v' => Some('z'), // zh initial → we emit 'z', final handles 'h'
            'i' => Some('c'), // ch initial
            'u' => Some('s'), // sh initial
            _ => None,
        }
    }

    /// Map a key to its pinyin final (complete syllable rhyme).
    /// Returns the full final string.
    fn map_final(ch: char) -> Option<&'static str> {
        Some(match ch {
            'a' => "a",
            'b' => "in",
            'c' => "ao",
            'd' => "ai",
            'e' => "e",
            'f' => "en",
            'g' => "eng",
            'h' => "ang",
            'i' => "i",
            'j' => "an",
            'k' => "ing",
            'l' => "uang",
            'm' => "ian",
            'n' => "iao",
            'o' => "uo",
            'p' => "ie",
            'q' => "iu",
            'r' => "uan",
            's' => "ong",
            't' => "ue",
            'u' => "u",
            'v' => "ui",
            'w' => "ei",
            'x' => "ia",
            'y' => "un",
            'z' => "ou",
            // Zero-initial finals (type 'o' first then the final key)
            _ => return None,
        })
    }

    /// Map a zero-initial final (for future use when buffer is implemented).
    #[allow(dead_code)]
    fn map_zero_final(ch: char) -> Option<&'static str> {
        Self::map_final(ch)
    }
}

impl KeyMapper for FlypyMapper {
    fn map(&self, event: &KeyEvent) -> KeyMapResult {
        let ch = match event.key {
            Key::Character(c) if c.is_ascii_lowercase() => c,
            _ => return KeyMapResult::default(),
        };

        if let Some(initial) = self.buffer {
            // We have a buffered initial, this key is the final
            let final_str = FlypyMapper::map_final(ch);
            let result = if let Some(fin) = final_str {
                // Double-pinyin special handling for zh/ch/sh
                let _actual_initial = match (initial, fin) {
                    ('z', _) => "zh",
                    ('c', _) => "ch",
                    ('s', _) => "sh",
                    _ => return KeyMapResult {
                        character: Some(initial),
                        consumed: false,
                    },
                };
                // Note: the full syllable construction happens downstream.
                // For now we emit characters that the segmentor can handle.
                KeyMapResult {
                    character: Some(ch),
                    consumed: false,
                }
            } else {
                KeyMapResult {
                    character: Some(ch),
                    consumed: false,
                }
            };
            // Buffer consumed
            return result;
        }

        // No buffer — check if this is the zero-initial marker
        if ch == 'o' {
            // 'o' is the zero-initial marker. Buffer it and wait for the final.
            // For simplicity, emit 'o' and let the segmentor handle it.
            return KeyMapResult {
                character: Some(ch),
                consumed: false,
            };
        }

        // Check if this is an initial consonant
        if FlypyMapper::map_initial(ch).is_some() {
            // Store in buffer, don't emit yet (real impl would buffer)
            // For now, just emit the character
            KeyMapResult {
                character: Some(ch),
                consumed: false,
            }
        } else {
            // Likely a final key without an initial
            KeyMapResult {
                character: Some(ch),
                consumed: false,
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn key(ch: char) -> KeyEvent {
        KeyEvent {
            key: Key::Character(ch),
            state: Default::default(),
        }
    }

    #[test]
    fn quanpin_passthrough() {
        let mapper = QuanPinMapper;
        assert_eq!(mapper.map(&key('n')).character, Some('n'));
        assert_eq!(mapper.map(&key('i')).character, Some('i'));
    }

    #[test]
    fn quanpin_non_alpha_ignored() {
        let mapper = QuanPinMapper;
        let result = mapper.map(&KeyEvent {
            key: Key::Backspace,
            state: Default::default(),
        });
        assert_eq!(result.character, None);
    }

    #[test]
    fn flypy_initial_keys() {
        let _mapper = FlypyMapper::new();
        assert_eq!(FlypyMapper::map_initial('b'), Some('b'));
        assert_eq!(FlypyMapper::map_initial('v'), Some('z')); // zh
        assert_eq!(FlypyMapper::map_initial('i'), Some('c')); // ch
        assert_eq!(FlypyMapper::map_initial('u'), Some('s')); // sh
    }

    #[test]
    fn flypy_final_keys() {
        assert_eq!(FlypyMapper::map_final('h'), Some("ang"));
        assert_eq!(FlypyMapper::map_final('j'), Some("an"));
        assert_eq!(FlypyMapper::map_final('q'), Some("iu"));
        assert_eq!(FlypyMapper::map_final('w'), Some("ei"));
        assert_eq!(FlypyMapper::map_final('p'), Some("ie"));
    }
}
