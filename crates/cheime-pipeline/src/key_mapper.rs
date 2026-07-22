//! Key mapper: translates physical key events into logical input characters.
//!
//! DRAFT §6 unified input model — first pipeline stage.
//! Different input schemes swap this component.
//!
//! Supports:
//! - QuanPin (全拼): direct passthrough
//! - DoublePinyin (双拼): configurable 2-key state machine with presets
//!   (Flypy/小鹤, MS/微软, Ziranma/自然码) and TSV-based custom mappings.

use cheime_model::{Key, KeyEvent};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyMapResult {
    pub characters: Vec<char>,
    pub consumed: bool,
}

pub trait KeyMapper: Send + Sync {
    fn map(&mut self, event: &KeyEvent) -> KeyMapResult;
}

// ── QuanPin (全拼) ─────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct QuanPinMapper;

impl KeyMapper for QuanPinMapper {
    fn map(&mut self, event: &KeyEvent) -> KeyMapResult {
        match event.key {
            Key::Character(c) if c.is_ascii_lowercase() => KeyMapResult {
                characters: vec![c],
                consumed: false,
            },
            _ => KeyMapResult::default(),
        }
    }
}

// ── Configurable Double-Pinyin mapper ────────────────────────────────
//  Generic state machine. Key→(initial, final, standalone) tables are configurable.
//  Supports Flypy, MS, Ziranma, and custom schemes loaded from TSV.

/// Key entry in a double-pinyin mapping table.
#[derive(Clone, Debug)]
pub struct KeyMapping {
    /// Characters produced as an initial consonant (empty for zero-initial keys).
    pub initial: Vec<char>,
    /// Characters produced as a final/vowel.
    pub final_chars: Vec<char>,
    /// If true: when initial is empty, emit final immediately as standalone syllable.
    /// If false: buffer as zero-initial, wait for second key (enables ad→ai, ah→ang).
    pub standalone: bool,
}

/// Configurable double-pinyin state machine.
#[derive(Clone, Debug)]
pub struct DoublePinyinMapper {
    state: u8,
    buffer: Vec<char>,
    mapping: HashMap<char, KeyMapping>,
}

impl DoublePinyinMapper {
    /// Build from entries: (key_char, initial_str, final_str, standalone).
    pub fn from_entries<'a>(
        entries: impl IntoIterator<Item = (char, &'a str, &'a str, bool)>,
    ) -> Self {
        let mapping: HashMap<char, KeyMapping> = entries
            .into_iter()
            .map(|(ch, init, fin, sa)| {
                (
                    ch,
                    KeyMapping {
                        initial: init.chars().collect(),
                        final_chars: fin.chars().collect(),
                        standalone: sa,
                    },
                )
            })
            .collect();
        Self {
            state: 0,
            buffer: Vec::new(),
            mapping,
        }
    }

    /// Load from TSV: `key<TAB>initial<TAB>final[<TAB>standalone]`.
    /// standalone defaults to true (compatible with Flypy behavior).
    pub fn from_tsv(tsv: &str) -> Result<Self, String> {
        let mut entries = Vec::new();
        for (i, line) in tsv.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = t.split('\t').collect();
            if parts.len() < 3 {
                return Err(format!(
                    "line {}: need key<TAB>initial<TAB>final[<TAB>sa]",
                    i + 1
                ));
            }
            let ch = parts[0]
                .chars()
                .next()
                .ok_or_else(|| format!("line {}: empty key", i + 1))?;
            let sa = parts
                .get(3)
                .copied()
                .map(|s| s == "1" || s == "true")
                .unwrap_or(true);
            entries.push((ch, parts[1], parts[2], sa));
        }
        Ok(Self::from_entries(entries))
    }

    // ── Presets ──────────────────────────────────────────────────────

    pub fn flypy() -> Self {
        Self::from_entries(FLYPY_ENTRIES.iter().copied())
    }
    pub fn ms_double() -> Self {
        Self::from_entries(MS_DOUBLE_ENTRIES.iter().copied())
    }
    pub fn ziranma() -> Self {
        Self::from_entries(ZIRANMA_ENTRIES.iter().copied())
    }
}

impl KeyMapper for DoublePinyinMapper {
    fn map(&mut self, event: &KeyEvent) -> KeyMapResult {
        let ch = match event.key {
            Key::Character(c) if c.is_ascii_lowercase() => c,
            _ => {
                self.state = 0;
                self.buffer.clear();
                return KeyMapResult::default();
            }
        };

        let Some(km) = self.mapping.get(&ch) else {
            return KeyMapResult {
                characters: vec![ch],
                consumed: false,
            };
        };

        if self.state == 1 {
            // Waiting for final
            self.state = 0;
            if !km.final_chars.is_empty() {
                let mut result = std::mem::take(&mut self.buffer);
                result.extend(&km.final_chars);
                return KeyMapResult {
                    characters: result,
                    consumed: false,
                };
            }
            let mut result = std::mem::take(&mut self.buffer);
            result.push(ch);
            return KeyMapResult {
                characters: result,
                consumed: false,
            };
        }

        // State 0: waiting for initial
        if !km.initial.is_empty() {
            self.buffer = km.initial.clone();
            self.state = 1;
            return KeyMapResult {
                characters: Vec::new(),
                consumed: true,
            };
        }

        // Zero-initial: standalone → emit now; non-standalone → buffer, wait
        if !km.final_chars.is_empty() {
            if km.standalone {
                return KeyMapResult {
                    characters: km.final_chars.clone(),
                    consumed: false,
                };
            }
            self.state = 1;
            return KeyMapResult {
                characters: Vec::new(),
                consumed: true,
            };
        }

        KeyMapResult {
            characters: vec![ch],
            consumed: false,
        }
    }
}

// ── Flypy (小鹤双拼) ────────────────────────────────────────────────

const FLYPY_ENTRIES: &[(char, &str, &str, bool)] = &[
    ('a', "", "a", true),
    ('b', "b", "in", false),
    ('c', "c", "ao", false),
    ('d', "d", "ai", false),
    ('e', "", "e", true),
    ('f', "f", "en", false),
    ('g', "g", "eng", false),
    ('h', "h", "ang", false),
    ('i', "ch", "i", false),
    ('j', "j", "an", false),
    ('k', "k", "ing", false),
    ('l', "l", "uang", false),
    ('m', "m", "ian", false),
    ('n', "n", "iao", false),
    ('o', "", "o", true),
    ('p', "p", "ie", false),
    ('q', "q", "iu", false),
    ('r', "r", "uan", false),
    ('s', "s", "ong", false),
    ('t', "t", "ue", false),
    ('u', "sh", "u", false),
    ('v', "zh", "ui", false),
    ('w', "w", "ei", false),
    ('x', "x", "ia", false),
    ('y', "y", "un", false),
    ('z', "z", "ou", false),
];

// ── MS Double Pinyin (微软双拼) ─────────────────────────────────────

const MS_DOUBLE_ENTRIES: &[(char, &str, &str, bool)] = &[
    ('a', "", "a", true),
    ('b', "b", "ou", false),
    ('c', "c", "iao", false),
    ('d', "d", "uang", false),
    ('e', "", "e", true),
    ('f', "f", "en", false),
    ('g', "g", "eng", false),
    ('h', "h", "ang", false),
    ('i', "ch", "i", false),
    ('j', "j", "ian", false),
    ('k', "k", "ao", false),
    ('l', "l", "ai", false),
    ('m', "m", "ian", false),
    ('n', "n", "in", false),
    ('o', "", "o", true),
    ('p', "p", "un", false),
    ('q', "q", "iu", false),
    ('r', "r", "uan", false),
    ('s', "s", "ong", false),
    ('t', "t", "ue", false),
    ('u', "sh", "u", false),
    ('v', "zh", "ue", false),
    ('w', "w", "ia", false),
    ('x', "x", "ie", false),
    ('y', "y", "uai", false),
    ('z', "z", "ei", false),
];

// ── Ziranma (自然码双拼) ────────────────────────────────────────────

const ZIRANMA_ENTRIES: &[(char, &str, &str, bool)] = &[
    ('a', "", "a", true),
    ('b', "b", "ou", false),
    ('c', "c", "iao", false),
    ('d', "d", "ua", false),
    ('e', "", "e", true),
    ('f', "f", "en", false),
    ('g', "g", "eng", false),
    ('h', "h", "ang", false),
    ('i', "ch", "i", false),
    ('j', "j", "an", false),
    ('k', "k", "ao", false),
    ('l', "l", "ai", false),
    ('m', "m", "ian", false),
    ('n', "n", "in", false),
    ('o', "", "o", true),
    ('p', "p", "un", false),
    ('q', "q", "iu", false),
    ('r', "r", "uan", false),
    ('s', "s", "ong", false),
    ('t', "t", "ve", false),
    ('u', "sh", "u", false),
    ('v', "zh", "ui", false),
    ('w', "w", "ia", false),
    ('x', "x", "ie", false),
    ('y', "y", "ing", false),
    ('z', "z", "ei", false),
];

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::KeyState;

    fn k(ch: char) -> KeyEvent {
        KeyEvent {
            key: Key::Character(ch),
            state: KeyState::default(),
        }
    }

    #[test]
    fn quanpin_passthrough() {
        let mut m = QuanPinMapper;
        let r = m.map(&k('n'));
        assert_eq!(r.characters, vec!['n']);
        assert!(!r.consumed);
    }

    #[test]
    fn flypy_vs_zhong() {
        let mut m = DoublePinyinMapper::flypy();
        let r1 = m.map(&k('v'));
        assert!(r1.consumed);
        assert!(r1.characters.is_empty());
        let r2 = m.map(&k('s'));
        assert_eq!(r2.characters, vec!['z', 'h', 'o', 'n', 'g']);
    }

    #[test]
    fn flypy_nj_nan() {
        let mut m = DoublePinyinMapper::flypy();
        m.map(&k('n'));
        assert_eq!(m.map(&k('j')).characters, vec!['n', 'a', 'n']);
    }

    #[test]
    fn flypy_standalone_a() {
        // 'a' has standalone=true → emits "a" immediately
        let r = DoublePinyinMapper::flypy().map(&k('a'));
        assert_eq!(r.characters, vec!['a']);
    }

    #[test]
    fn flypy_backspace_resets_state() {
        let mut m = DoublePinyinMapper::flypy();
        m.map(&k('v'));
        assert_eq!(m.state, 1);
        m.map(&KeyEvent {
            key: Key::Backspace,
            state: KeyState::default(),
        });
        assert_eq!(m.state, 0);
        assert!(m.buffer.is_empty());
    }

    #[test]
    fn custom_non_standalone_ad_to_ai() {
        // 'a' with standalone=false → zero-initial that waits
        let entries = vec![('a', "", "a", false), ('d', "d", "ai", false)];
        let mut m = DoublePinyinMapper::from_entries(entries);
        m.map(&k('a')); // zero-initial, buffers nothing, consumed
        let r = m.map(&k('d')); // final "ai"
        assert_eq!(r.characters, vec!['a', 'i']);
    }

    #[test]
    fn ms_double_has_different_mapping() {
        let mut m = DoublePinyinMapper::ms_double();
        m.map(&k('m'));
        assert_eq!(m.map(&k('b')).characters, vec!['m', 'o', 'u']);
    }

    #[test]
    fn custom_tsv_mapping() {
        let tsv = "# custom\nb\tb\tou\tfalse\nm\tm\tao\tfalse\n";
        let mut m = DoublePinyinMapper::from_tsv(tsv).unwrap();
        m.map(&k('b'));
        assert_eq!(m.map(&k('m')).characters, vec!['b', 'a', 'o']);
    }

    #[test]
    fn unknown_key_returns_empty() {
        // Non-lowercase characters reset state and return empty (consumed by pipeline elsewhere)
        let r = DoublePinyinMapper::flypy().map(&k(';'));
        assert!(r.characters.is_empty());
    }
}
