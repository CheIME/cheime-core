//! Dictionary-backed translator: code → candidates from a CompiledIndex.
//!
//! Also includes a no-op PassthroughTranslator for when the segmentor
//! cannot split the composition (e.g. non-pinyin input).

use crate::decoder::{Decoder, DecoderOptions, Lexicon, ResolvedCandidate};
use crate::segmentation::SegmentationGraph;
use crate::{CodeSegment, Translator};
use cheime_dictionary::{CompiledIndex, LexiconEntry};
use cheime_model::Candidate;
use std::sync::Arc;

/// Translates segments by querying a compiled dictionary index.
#[derive(Clone)]
pub struct DictTranslator {
    name: String,
    index: Arc<CompiledIndex>,
    lexicons: Vec<Arc<dyn Lexicon>>,
    options: DecoderOptions,
}

impl DictTranslator {
    pub fn new(name: impl Into<String>, index: Arc<CompiledIndex>) -> Self {
        let lexicon: Arc<dyn Lexicon> = index.clone();
        Self {
            name: name.into(),
            index,
            lexicons: vec![lexicon],
            options: DecoderOptions::default(),
        }
    }

    pub fn with_options(mut self, options: DecoderOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_user_store(mut self, store: Arc<PLMutex<UserStore>>) -> Self {
        self.lexicons.insert(0, Arc::new(UserLexicon { store }));
        self
    }
}

impl Translator for DictTranslator {
    fn name(&self) -> &str {
        &self.name
    }

    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate> {
        let code = segments
            .iter()
            .map(|s| s.code.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        match segments.len() {
            0 => vec![],
            1 if code.len() == 1 => self.index.query_prefix(&code, 100),
            1 => self.index.query(&code),
            _ => {
                let results = self.index.query_prefix(&code, 10);
                if !results.is_empty() {
                    return results;
                }
                vec![]
            }
        }
    }

    fn translate_graph(&self, graph: &SegmentationGraph) -> Vec<ResolvedCandidate> {
        Decoder::with_options(self.lexicons.clone(), self.options).decode("", graph)
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
    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate> {
        if segments.is_empty() {
            return vec![];
        }
        let text = segments
            .iter()
            .map(|s| s.code.as_str())
            .collect::<Vec<_>>()
            .join("");
        vec![Candidate {
            id: cheime_model::CandidateId::new(1),
            text,
            annotation: None,
            source: String::from("passthrough"),
            is_emoji: false,
        }]
    }
}
// ── User dictionary ────────────────────────────────────────────────

use cheime_user_data::UserStore;
use parking_lot::Mutex as PLMutex;

/// Translates segments by querying the user's learned words.
#[derive(Debug)]
pub struct UserDictTranslator {
    store: Arc<PLMutex<UserStore>>,
}

#[derive(Debug)]
struct UserLexicon {
    store: Arc<PLMutex<UserStore>>,
}

impl Lexicon for UserLexicon {
    fn exact(&self, code: &str) -> Vec<LexiconEntry> {
        self.entries(self.store.lock().query(code), false)
    }

    fn prefix(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        let mut entries = self.entries(self.store.lock().query_prefix(code), true);
        entries.truncate(limit);
        entries
    }
}

impl UserLexicon {
    fn entries(
        &self,
        candidates: Vec<cheime_user_data::UserCandidate>,
        completion: bool,
    ) -> Vec<LexiconEntry> {
        candidates
            .into_iter()
            .map(|candidate| LexiconEntry {
                text: candidate.text,
                code: candidate.code,
                weight: candidate.frequency,
                source: String::from("user_dict"),
                completion,
            })
            .collect()
    }
}

impl UserDictTranslator {
    pub fn new(store: Arc<PLMutex<UserStore>>) -> Self {
        Self { store }
    }
}

impl Translator for UserDictTranslator {
    fn name(&self) -> &str {
        "user_dict"
    }
    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate> {
        let code = segments
            .iter()
            .map(|s| s.code.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let store = self.store.lock();
        let user_cands = store.query(&code);
        user_cands
            .into_iter()
            .enumerate()
            .map(|(i, uc)| Candidate {
                id: cheime_model::CandidateId::new(3_000_000 + i as u64),
                text: uc.text,
                annotation: Some(format!("{}×{}", code, uc.frequency)),
                source: String::from("user_dict"),
                is_emoji: false,
            })
            .collect()
    }

    fn translate_graph(&self, graph: &SegmentationGraph) -> Vec<ResolvedCandidate> {
        if graph
            .edges()
            .all(|edge| edge.kind == crate::segmentation::SyllableKind::Raw)
        {
            let path = graph.primary_path();
            let code = path
                .iter()
                .map(|segment| segment.code.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            return self
                .translate(&path)
                .into_iter()
                .map(|candidate| {
                    ResolvedCandidate::from_display(
                        candidate,
                        crate::segmentation::InputSpan::new(0, graph.input_len()),
                        code.clone(),
                        true,
                        0,
                    )
                })
                .collect();
        }
        let lexicon: Arc<dyn Lexicon> = Arc::new(UserLexicon {
            store: Arc::clone(&self.store),
        });
        Decoder::new(vec![lexicon]).decode("", graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segmentor;
    use crate::segmentor::PinyinSegmentor;
    use cheime_dictionary::{CompiledIndex, DictEntry};
    use cheime_model::DeploymentGeneration;
    use std::sync::Arc;

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
        Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1)))
    }

    #[test]
    fn dict_translator_returns_candidates() {
        let translator = DictTranslator::new("test", test_index());
        let segment = CodeSegment {
            code: "ni".into(),
            tag: "pinyin".into(),
        };
        let candidates = translator.translate(&[segment]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].text, "你");
    }

    #[test]
    fn dict_translator_completes_incomplete_nih() {
        let index = Arc::new(CompiledIndex::build(
            vec![
                DictEntry {
                    text: "你好".into(),
                    code: "ni hao".into(),
                    weight: Some(200),
                    stem: None,
                },
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
            ],
            DeploymentGeneration::new(1),
        ));
        let translator = DictTranslator::new("test", index);
        let graph = PinyinSegmentor::new().segment("nih");
        let candidates = translator.translate_graph(&graph);
        assert!(candidates.iter().any(|candidate| candidate.text == "你好"));
    }

    #[test]
    fn passthrough_returns_code_as_text() {
        let t = PassthroughTranslator;
        let seg = CodeSegment {
            code: "hello".into(),
            tag: "unknown".into(),
        };
        let candidates = t.translate(&[seg]);
        assert_eq!(candidates[0].text, "hello");
    }
}
