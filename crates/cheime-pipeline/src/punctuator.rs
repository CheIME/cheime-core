//! Punctuator processor — Rime-compatible symbol mapping.
//!
//! CheIME advantage: the punctuator is a typed Processor with a
//! well-defined config schema. Rime's punctuator parses strings
//! at runtime with ad-hoc key mapping.
//!
//! ## Behaviour
//! - Symbol keys (`|`, `$`, `.`, etc.) are intercepted before composition.
//! - Single-commit: configured value committed immediately (e.g. `.` → `。`).
//! - Candidate list: configured alternatives shown as candidates (e.g. `|` → `·`, `｜`, `§`, `¦`).
//! - Pair: opening + closing committed (e.g. `"` → `""`).
//! - After a digit, `.` and `:` bypass the punctuator (intent: 3.14, 12:30).

use crate::{PipelineError, PipelineIntent, Processor, ProcessorOutput};
use cheime_config::schema::PunctuatorConfig;
use cheime_model::{Candidate, CandidateId, Key, KeyEvent};
use std::collections::BTreeMap;

// ── Punct Action ──────────────────────────────────────────────────

/// What happens when a punctuation key is pressed.
#[derive(Clone, Debug)]
enum PunctAction {
    /// Commit the given text immediately.
    Commit(String),
    /// Show these texts as candidates.
    Candidates(Vec<String>),
    /// Commit a paired open/close (e.g. `""`).
    /// Currently committed as concatenation; cursor placement deferred.
    Pair(String, String),
}

// ── Processor ─────────────────────────────────────────────────────

pub struct PunctProcessor {
    /// Inner processor for non-punctuation keys.
    inner: Box<dyn Processor>,
    /// The active punctuator map (full_shape or half_shape).
    active: BTreeMap<char, PunctAction>,
    /// Whether the previous character was a digit (ASCII 0-9).
    last_was_digit: bool,
    /// Counter for assigning unique CandidateIds.
    id_counter: u64,
}

impl PunctProcessor {
    /// Build from a PunctuatorConfig, selecting the full_shape or half_shape map.
    /// Wraps the given inner processor for non-punctuation key handling.
    pub fn new(config: &PunctuatorConfig, half_shape: bool, inner: Box<dyn Processor>) -> Self {
        let raw_map = if half_shape {
            &config.half_shape
        } else {
            &config.full_shape
        };
        let active = parse_map(raw_map);
        Self {
            inner,
            active,
            last_was_digit: false,
            id_counter: 4_000_000,
        }
    }

    fn next_id(&mut self) -> CandidateId {
        let id = CandidateId::new(self.id_counter);
        self.id_counter += 1;
        id
    }
}

impl Processor for PunctProcessor {
    fn process(
        &mut self,
        composition: &str,
        event: &KeyEvent,
    ) -> Result<ProcessorOutput, PipelineError> {
        let ch = match event.key {
            Key::Character(c) => c,
            _ => {
                self.last_was_digit = false;
                return self.inner.process(composition, event);
            }
        };

        let prev_was_digit = self.last_was_digit;
        self.last_was_digit = ch.is_ascii_digit();

        // After a digit, `.` and `:` pass through as regular characters
        if prev_was_digit && (ch == '.' || ch == ':') {
            let mut next = composition.to_owned();
            next.push(ch);
            return Ok(ProcessorOutput {
                composition: next,
                intent: PipelineIntent::None,
                consumed: false,
                inject_candidates: vec![],
            });
        }

        // Look up in punctuator map
        if let Some(action) = self.active.get(&ch).cloned() {
            match action {
                PunctAction::Commit(text) => Ok(ProcessorOutput {
                    composition: composition.to_owned(),
                    intent: PipelineIntent::CommitText(text),
                    consumed: true,
                    inject_candidates: vec![],
                }),
                PunctAction::Pair(open, close) => Ok(ProcessorOutput {
                    composition: composition.to_owned(),
                    intent: PipelineIntent::CommitText(format!("{open}{close}")),
                    consumed: true,
                    inject_candidates: vec![],
                }),
                PunctAction::Candidates(texts) => {
                    let candidates: Vec<Candidate> = texts
                        .into_iter()
                        .map(|t| Candidate {
                            id: self.next_id(),
                            text: t,
                            annotation: None,
                            source: format!("punct:{ch}"),
                            is_emoji: false,
                        })
                        .collect();
                    Ok(ProcessorOutput {
                        composition: ch.to_string(),
                        intent: PipelineIntent::None,
                        consumed: false,
                        inject_candidates: candidates,
                    })
                }
            }
        } else {
            self.inner.process(composition, event)
        }
    }
}
// ── Config parsing ────────────────────────────────────────────────

fn parse_map(raw: &BTreeMap<String, serde_json::Value>) -> BTreeMap<char, PunctAction> {
    let mut map = BTreeMap::new();
    for (key, value) in raw {
        let ch = match key.as_str() {
            " " => ' ',
            s if s.len() == 1 => s.chars().next().unwrap(),
            _ => continue, // skip non-single-char keys
        };
        let action = parse_action(value);
        map.insert(ch, action);
    }
    map
}

fn parse_action(value: &serde_json::Value) -> PunctAction {
    match value {
        // String → single commit (or if string looks like a symbol, commit it)
        serde_json::Value::String(s) => PunctAction::Commit(s.clone()),

        // Array → candidates
        serde_json::Value::Array(arr) => {
            let texts: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            PunctAction::Candidates(texts)
        }

        // Object → {commit: "x"} or {pair: ["x", "y"]}
        serde_json::Value::Object(obj) => {
            if let Some(commit_val) = obj.get("commit") {
                if let Some(s) = commit_val.as_str() {
                    return PunctAction::Commit(s.to_owned());
                }
            }
            if let Some(pair_val) = obj.get("pair") {
                if let Some(arr) = pair_val.as_array() {
                    if arr.len() >= 2 {
                        let open = arr[0].as_str().unwrap_or("").to_owned();
                        let close = arr[1].as_str().unwrap_or("").to_owned();
                        return PunctAction::Pair(open, close);
                    }
                }
            }
            // Fallback: treat as commit of the key itself
            PunctAction::Commit(String::new())
        }

        _ => PunctAction::Commit(String::new()),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

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

    fn dummy_processor() -> Box<dyn Processor> {
        Box::new(crate::processor::DefaultProcessor::new())
    }

    fn test_config() -> PunctuatorConfig {
        let mut full = BTreeMap::new();
        full.insert(".".into(), serde_json::json!({"commit": "。"}));
        full.insert(":".into(), serde_json::json!({"commit": "："}));
        full.insert("|".into(), serde_json::json!(["·", "｜", "§", "¦"]));
        full.insert(
            "\"".into(),
            serde_json::json!({"pair": ["\u{201c}", "\u{201d}"]}),
        );
        full.insert("$".into(), serde_json::json!(["￥", "$", "€"]));

        let half = BTreeMap::new();
        PunctuatorConfig {
            full_shape: full,
            half_shape: half,
        }
    }

    #[test]
    fn dot_commits_fullwidth_period() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let out = p.process("", &k('.')).unwrap();
        assert!(matches!(out.intent, PipelineIntent::CommitText(ref t) if t == "。"));
    }

    #[test]
    fn pipe_shows_candidates() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let out = p.process("", &k('|')).unwrap();
        assert_eq!(out.inject_candidates.len(), 4);
        assert!(out.inject_candidates.iter().any(|c| c.text == "·"));
        assert!(out.inject_candidates.iter().any(|c| c.text == "｜"));
        assert_eq!(out.composition, "|");
    }

    #[test]
    fn after_digit_dot_passes_through() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        // First, type a digit (via inner processor)
        let out = p.process("", &k('3')).unwrap();
        assert_eq!(out.composition, "3");
        // Now `.` after digit should pass through
        let out = p.process("3", &k('.')).unwrap();
        assert_eq!(out.composition, "3."); // half-width, not full-width。
    }

    #[test]
    fn after_digit_colon_passes_through() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let _ = p.process("", &k('1'));
        let _ = p.process("1", &k('2')).unwrap();
        let out = p.process("12", &k(':')).unwrap();
        assert_eq!(out.composition, "12:"); // half-width, for time input
    }

    #[test]
    fn normal_letter_not_intercepted() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let out = p.process("", &k('n')).unwrap();
        assert_eq!(out.composition, "n");
    }

    #[test]
    fn dollar_shows_candidates() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let out = p.process("", &k('$')).unwrap();
        assert_eq!(out.composition, "$");
        assert!(out.inject_candidates.iter().any(|c| c.text == "￥"));
        assert!(out.inject_candidates.iter().any(|c| c.text == "$"));
    }

    #[test]
    fn quote_commits_pair() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let out = p.process("", &k('"')).unwrap();
        assert!(matches!(out.intent, PipelineIntent::CommitText(ref t) if t == "\u{201c}\u{201d}"));
    }

    #[test]
    fn digit_tracking_resets_on_non_digit() {
        let mut p = PunctProcessor::new(&test_config(), false, dummy_processor());
        let _ = p.process("", &k('3'));
        let _ = p.process("3", &k('n')); // letter resets digit tracking
        let out = p.process("3n", &k('.')).unwrap();
        assert!(matches!(out.intent, PipelineIntent::CommitText(ref t) if t == "。"));
    }
}
