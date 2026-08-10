//! Key mapper: translates physical key events into logical input characters.
//!
//! This component is reserved for PHYSICAL layout remapping (Dvorak,
//! Colemak, platform-specific keys). Input coding schemes — quanpin and
//! double pinyin — are handled by the segmentor stage (`segmentor.rs` /
//! `double_pinyin.rs`), driven by the `engine.input` schema. Double pinyin
//! was historically implemented here as a stateful mapper that expanded raw
//! codes into pinyin strings; that path was removed when the native
//! `DoublePinyinSegmentor` landed (see docs/superpowers/plans/).

use cheime_model::{Key, KeyEvent};

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
        let mut mapper = QuanPinMapper;
        let result = mapper.map(&key('v'));
        assert_eq!(result.characters, vec!['v']);
        assert!(!result.consumed);
    }

    #[test]
    fn non_character_events_are_ignored() {
        let mut mapper = QuanPinMapper;
        let result = mapper.map(&KeyEvent {
            key: Key::Backspace,
            state: Default::default(),
        });
        assert!(result.characters.is_empty());
    }
}
