//! Default key processor: character append, backspace, enter/escape/space.
//!
//! Handles the composition editing logic that was previously embedded
//! in BuiltinPipeline.

use crate::{PipelineError, PipelineIntent, Processor, ProcessorOutput};
use cheime_model::{Key, KeyEvent};

#[derive(Clone, Debug, Default)]
pub struct DefaultProcessor;

impl DefaultProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for DefaultProcessor {
    fn process(
        &mut self,
        composition: &str,
        event: &KeyEvent,
    ) -> Result<ProcessorOutput, PipelineError> {
        let mut next = composition.to_owned();
        let (intent, consumed) = match event.key {
            Key::Character(c) if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '\'' => {
                next.push(c);
                (PipelineIntent::None, false)
            }
            Key::Character(c) => {
                return Err(PipelineError::UnsupportedCharacter(c));
            }
            Key::Backspace => {
                next.pop();
                (PipelineIntent::None, false)
            }
            Key::Escape => (PipelineIntent::Cancel, true),
            Key::Enter => (PipelineIntent::CommitRaw, true),
            Key::Space => (PipelineIntent::CommitHighlighted, true),
        };

        Ok(ProcessorOutput {
            composition: next,
            intent,
            consumed,
            inject_candidates: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PipelineError, PipelineIntent};
    use cheime_model::{Key, KeyEvent, KeyState};
    fn key(k: Key) -> KeyEvent {
        KeyEvent {
            key: k,
            state: KeyState::default(),
        }
    }

    fn processor() -> DefaultProcessor {
        DefaultProcessor::new()
    }

    #[test]
    fn lowercase_char_appends() {
        let out = processor().process("", &key(Key::Character('n'))).unwrap();
        assert_eq!(out.composition, "n");
        assert!(!out.consumed);
    }

    #[test]
    fn apostrophe_appends_as_a_syllable_boundary() {
        let out = processor()
            .process("xi", &key(Key::Character('\'')))
            .unwrap();
        assert_eq!(out.composition, "xi'");
        assert!(!out.consumed);
    }

    #[test]
    fn uppercase_rejected() {
        let err = processor()
            .process("", &key(Key::Character('N')))
            .unwrap_err();
        assert!(matches!(err, PipelineError::UnsupportedCharacter('N')));
    }

    #[test]
    fn backspace_removes_one_char() {
        let out = processor().process("ni", &key(Key::Backspace)).unwrap();
        assert_eq!(out.composition, "n");
    }

    #[test]
    fn enter_requests_commit() {
        let out = processor().process("ni", &key(Key::Enter)).unwrap();
        assert_eq!(out.intent, PipelineIntent::CommitRaw);
        assert!(out.consumed);
        assert_eq!(out.composition, "ni"); // composition unchanged
    }

    #[test]
    fn escape_cancels() {
        let out = processor().process("ni", &key(Key::Escape)).unwrap();
        assert_eq!(out.intent, PipelineIntent::Cancel);
        assert!(out.consumed);
    }
}
