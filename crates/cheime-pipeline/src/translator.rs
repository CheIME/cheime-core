//! Dictionary-backed translator: code → candidates from a CompiledIndex.
//!
//! Also includes a no-op PassthroughTranslator for when the segmentor
//! cannot split the composition (e.g. non-pinyin input).

use crate::{CodeSegment, Translator};
use cheime_dictionary::CompiledIndex;
use cheime_model::{Candidate, CandidateId};
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
                let mut results = self.index.query(&code);
                for candidate in &mut results {
                    candidate.source = format!("dict:exact:{}", candidate.source);
                }
                for candidate in self.index.query_prefix(&code, 10) {
                    if !results
                        .iter()
                        .any(|existing| existing.text == candidate.text)
                    {
                        results.push(candidate);
                    }
                }

                let mut per_seg: Vec<Vec<Candidate>> = Vec::with_capacity(segments.len());
                for seg in segments {
                    let seg_code = &seg.code;
                    let seg_results = if seg_code.len() == 1 {
                        self.index.query_prefix(seg_code, 5)
                    } else {
                        self.index.query(seg_code)
                    };
                    per_seg.push(seg_results);
                }
                // Produce concatenated candidates: take top-N from each segment
                // and generate cross-product combinations (up to limit)
                let limit = 10;
                let mut combined = Vec::new();
                let concat_text: String = per_seg
                    .iter()
                    .zip(segments.iter())
                    .map(|(results, seg)| {
                        if results.is_empty() {
                            seg.code.as_str() // raw code as fallback
                        } else {
                            results[0].text.as_str()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !concat_text.is_empty() {
                    combined.push(Candidate {
                        id: CandidateId::new(1),
                        text: concat_text,
                        annotation: None,
                        source: "dict:concat".to_string(),
                        is_emoji: false,
                    });
                }
                for seg_results in &per_seg {
                    for c in seg_results {
                        if combined.len() >= limit {
                            break;
                        }
                        if !combined.iter().any(|existing| existing.text == c.text) {
                            combined.push(c.clone());
                        }
                    }
                    if combined.len() >= limit {
                        break;
                    }
                }
                for candidate in combined {
                    if !results
                        .iter()
                        .any(|existing| existing.text == candidate.text)
                    {
                        results.push(candidate);
                    }
                }
                results
            }
        }
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
}

#[cfg(test)]
#[path = "translator_tests.rs"]
mod tests;
