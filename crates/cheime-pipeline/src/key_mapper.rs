//! Key mapper: translates physical key events into logical input characters.
//!
//! DRAFT §6 unified input model — first pipeline stage.
//! Different input schemes swap this component.

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
            Key::Character(ch) if ch.is_ascii_lowercase() => KeyMapResult {
                characters: vec![ch], consumed: false,
            },
            _ => KeyMapResult::default(),
        }
    }
}

// ── Flypy (小鹤双拼) state machine ──────────────────────────────────
//
// 2-keystroke → full pinyin syllable.
// State 0: waiting for initial consonant.
// State 1: waiting for final.

#[derive(Clone, Debug, Default)]
pub struct FlypyMapper {
    state: u8,
    /// Buffered initial characters (e.g., "zh" from 'v').
    buffer: Vec<char>,
}

impl FlypyMapper {
    pub fn new() -> Self { Self { state: 0, buffer: Vec::new() } }

    fn initial_chars(ch: char) -> Option<Vec<char>> {
        Some(match ch {
            'b' => vec!['b'], 'p' => vec!['p'], 'm' => vec!['m'], 'f' => vec!['f'],
            'd' => vec!['d'], 't' => vec!['t'], 'n' => vec!['n'], 'l' => vec!['l'],
            'g' => vec!['g'], 'k' => vec!['k'], 'h' => vec!['h'],
            'j' => vec!['j'], 'q' => vec!['q'], 'x' => vec!['x'],
            'r' => vec!['r'], 'z' => vec!['z'], 'c' => vec!['c'], 's' => vec!['s'],
            'y' => vec!['y'], 'w' => vec!['w'],
            'v' => vec!['z', 'h'], // zh
            'i' => vec!['c', 'h'], // ch
            'u' => vec!['s', 'h'], // sh
            _ => return None,
        })
    }

    fn final_chars(ch: char) -> Option<Vec<char>> {
        Some(match ch {
            'a' => vec!['a'], 'e' => vec!['e'], 'i' => vec!['i'],
            'o' => vec!['o'], 'u' => vec!['u'],
            'b' => vec!['i', 'n'], 'c' => vec!['a', 'o'], 'd' => vec!['a', 'i'],
            'f' => vec!['e', 'n'], 'g' => vec!['e', 'n', 'g'], 'h' => vec!['a', 'n', 'g'],
            'j' => vec!['a', 'n'], 'k' => vec!['i', 'n', 'g'], 'l' => vec!['u', 'a', 'n', 'g'],
            'm' => vec!['i', 'a', 'n'], 'n' => vec!['i', 'a', 'o'], 'p' => vec!['i', 'e'],
            'q' => vec!['i', 'u'], 'r' => vec!['u', 'a', 'n'], 's' => vec!['o', 'n', 'g'],
            't' => vec!['u', 'e'], 'v' => vec!['u', 'i'], 'w' => vec!['e', 'i'],
            'x' => vec!['i', 'a'], 'y' => vec!['u', 'n'], 'z' => vec!['o', 'u'],
            _ => return None,
        })
    }
}

impl KeyMapper for FlypyMapper {
    fn map(&mut self, event: &KeyEvent) -> KeyMapResult {
        let ch = match event.key {
            Key::Character(c) if c.is_ascii_lowercase() => c,
            _ => return KeyMapResult::default(),
        };

        if self.state == 1 {
            // Waiting for final
            self.state = 0;
            if let Some(mut fin) = FlypyMapper::final_chars(ch) {
                let mut result = std::mem::take(&mut self.buffer);
                result.append(&mut fin);
                return KeyMapResult { characters: result, consumed: false };
            }
            // Invalid final — flush buffer + current key as-is
            let mut result = std::mem::take(&mut self.buffer);
            result.push(ch);
            return KeyMapResult { characters: result, consumed: false };
        }

        // State 0: waiting for initial
        if let Some(init) = FlypyMapper::initial_chars(ch) {
            // Buffer the initial, don't emit yet
            self.buffer = init;
            self.state = 1;
            return KeyMapResult { characters: Vec::new(), consumed: true };
        }

        // Not an initial — try as standalone final (zero-initial syllables like "a", "ai", "an"...)
        if let Some(fin) = FlypyMapper::final_chars(ch) {
            return KeyMapResult { characters: fin, consumed: false };
        }

        // Pass through
        KeyMapResult { characters: vec![ch], consumed: false }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::KeyState;

    fn k(ch: char) -> KeyEvent { KeyEvent { key: Key::Character(ch), state: KeyState::default() } }

    #[test]
    fn quanpin_passthrough() {
        let mut m = QuanPinMapper;
        assert_eq!(m.map(&k('n')).characters, vec!['n']);
        assert_eq!(m.map(&k('i')).characters, vec!['i']);
    }

    #[test]
    fn flypy_vs_zhong() {
        let mut m = FlypyMapper::new();
        // 'v' → zh initial (buffered, state→1)
        let r1 = m.map(&k('v'));
        assert!(r1.characters.is_empty());
        assert!(r1.consumed);
        // 's' → ong final → emit "zh" + "ong"
        let r2 = m.map(&k('s'));
        assert_eq!(r2.characters, vec!['z', 'h', 'o', 'n', 'g']);
    }

    #[test]
    fn flypy_nj_nan() {
        let mut m = FlypyMapper::new();
        m.map(&k('n'));
        let r2 = m.map(&k('j'));
        assert_eq!(r2.characters, vec!['n', 'a', 'n']);
    }

    #[test]
    fn flypy_zero_initial_a() {
        let mut m = FlypyMapper::new();
        // 'a' as standalone final (zero-initial)
        let r = m.map(&k('a'));
        assert_eq!(r.characters, vec!['a']);
    }
}
