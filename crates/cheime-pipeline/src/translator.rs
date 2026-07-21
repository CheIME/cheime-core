//! Dictionary-backed translator: code → candidates from a CompiledIndex.
//!
//! Also includes a no-op PassthroughTranslator for when the segmentor
//! cannot split the composition (e.g. non-pinyin input).

use crate::{CodeSegment, Translator};
use cheime_dictionary::CompiledIndex;
use cheime_model::Candidate;
use std::sync::Arc;

/// Translates segments by querying a compiled dictionary index.
#[derive(Clone, Debug)]
pub struct DictTranslator {
    name: String,
    index: Arc<CompiledIndex>,
}

impl DictTranslator {
    pub fn new(name: impl Into<String>, index: Arc<CompiledIndex>) -> Self {
        Self {
            name: name.into(),
            index,
        }
    }
}

impl Translator for DictTranslator {
    fn name(&self) -> &str {
        &self.name
    }

    fn translate(&self, segment: &CodeSegment) -> Vec<Candidate> {
        self.index.query(&segment.code)
    }
}

// ── Pass-through (no segmentation was possible) ─────────────────────

/// Returns the entire composition as a single candidate.
/// Used as fallback when no dictionary matches.
#[derive(Clone, Debug, Default)]
pub struct PassthroughTranslator;

impl Translator for PassthroughTranslator {
    fn name(&self) -> &str {
        "passthrough"
    }

    fn translate(&self, segment: &CodeSegment) -> Vec<Candidate> {
        vec![Candidate {
            id: cheime_model::CandidateId::new(1),
            text: segment.code.clone(),
            annotation: None,
            source: String::from("passthrough"),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_dictionary::DictEntry;
    use cheime_model::DeploymentGeneration;
    fn test_index() -> Arc<CompiledIndex> {
        let entries = vec![
            DictEntry {
                text: "你".into(),
                code: "ni".into(),
                weight: Some(100),
                stem: None,
            },
            DictEntry {
                text: "好".into(),
                code: "hao".into(),
                weight: Some(100),
                stem: None,
            },
        ];
        Arc::new(CompiledIndex::build(
            entries,
            DeploymentGeneration::new(1),
        ))
    }

    #[test]
    fn dict_translator_returns_candidates() {
        let translator = DictTranslator::new("test", test_index());
        let segment = CodeSegment {
            code: "ni".into(),
            tag: "pinyin".into(),
        };
        let candidates = translator.translate(&segment);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].text, "你");
    }

    #[test]
    fn passthrough_returns_code_as_text() {
        let t = PassthroughTranslator;
        let seg = CodeSegment {
            code: "hello".into(),
            tag: "unknown".into(),
        };
        let candidates = t.translate(&seg);
        assert_eq!(candidates[0].text, "hello");
    }
}
