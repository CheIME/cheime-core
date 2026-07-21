//! Convenience pipeline builder from inline entries.
//!
//! BuiltinPipeline composes DefaultProcessor + simple dictionary lookup
//! (no segmentation). For a full pinyin pipeline with segmentation,
//! use `ComposablePipeline` directly with `PinyinSegmentor`.

use crate::{InputPipeline, PipelineError, PipelineIntent, PipelineUpdate};
use cheime_model::{Candidate, CandidateId, KeyEvent};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Entry {
    text: String,
    weight: i64,
}

/// A simple pipeline with inline entries and no segmentation.
/// Kept for backward compatibility with tests and simple use cases.
#[derive(Clone, Debug, Default)]
pub struct BuiltinPipeline {
    entries: BTreeMap<String, Vec<Entry>>,
}

impl BuiltinPipeline {
    pub fn new(entries: impl IntoIterator<Item = (String, String, i64)>) -> Self {
        let mut grouped: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
        for (code, text, weight) in entries {
            grouped
                .entry(code)
                .or_default()
                .push(Entry { text, weight });
        }
        for values in grouped.values_mut() {
            values.sort_by(|left, right| {
                right
                    .weight
                    .cmp(&left.weight)
                    .then_with(|| left.text.cmp(&right.text))
            });
        }
        Self { entries: grouped }
    }

    fn candidates(&self, composition: &str) -> Vec<Candidate> {
        self.entries
            .get(composition)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, entry)| Candidate {
                id: CandidateId::new(index as u64 + 1),
                text: entry.text.clone(),
                annotation: Some(composition.to_owned()),
                source: String::from("builtin"),
            })
            .collect()
    }
}

impl InputPipeline for BuiltinPipeline {
    fn apply(
        &self,
        composition: &str,
        event: &KeyEvent,
    ) -> Result<PipelineUpdate, PipelineError> {
        use cheime_model::Key;
        let mut next = composition.to_owned();
        let intent = match event.key {
            Key::Character(character) if character.is_ascii_lowercase() => {
                next.push(character);
                PipelineIntent::None
            }
            Key::Character(character) => {
                return Err(PipelineError::UnsupportedCharacter(character));
            }
            Key::Backspace => {
                next.pop();
                PipelineIntent::None
            }
            Key::Escape => PipelineIntent::Cancel,
            Key::Enter | Key::Space => PipelineIntent::CommitHighlighted,
        };

        Ok(PipelineUpdate {
            candidates: self.candidates(&next),
            composition: next,
            intent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{Key, KeyEvent, KeyState};

    fn key(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            state: KeyState::default(),
        }
    }

    fn pipeline() -> BuiltinPipeline {
        BuiltinPipeline::new([(String::from("ni"), String::from("你"), 10)])
    }

    #[test]
    fn character_extends_composition_and_returns_ranked_candidates() {
        let p = pipeline();
        let update = p.apply("", &key(Key::Character('n'))).unwrap();
        assert_eq!(update.composition, "n");
        assert!(update.candidates.is_empty());
    }

    #[test]
    fn backspace_removes_one_character() {
        let p = pipeline();
        let update = p.apply("ni", &key(Key::Backspace)).unwrap();
        assert_eq!(update.composition, "n");
    }

    #[test]
    fn enter_requests_commit_without_changing_composition() {
        let p = pipeline();
        let update = p.apply("ni", &key(Key::Enter)).unwrap();
        assert_eq!(update.composition, "ni");
        assert_eq!(update.intent, PipelineIntent::CommitHighlighted);
    }

    #[test]
    fn escape_requests_cancel() {
        let p = pipeline();
        let update = p.apply("ni", &key(Key::Escape)).unwrap();
        assert_eq!(update.intent, PipelineIntent::Cancel);
    }
}
