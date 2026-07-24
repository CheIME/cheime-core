#![allow(clippy::needless_range_loop)]
#![forbid(unsafe_code)]

mod builtin;
pub mod decoder;
pub mod emoji;
pub mod factory;
pub mod filter;
pub mod key_mapper;
pub mod learning;
pub mod normalizer;
pub mod processor;
pub mod punctuator;
pub mod ranker;
pub mod segmentation;
pub mod segmentor;
pub mod simplifier;
pub mod translator;
use crate::decoder::{ResolvedCandidate, SelectedLexeme};
use crate::key_mapper::KeyMapper;
pub use crate::learning::CommitRecord;
use crate::normalizer::CodeNormalizer;
use crate::segmentation::SegmentationGraph;
pub use builtin::BuiltinPipeline;
use cheime_model::CandidateId;
use cheime_model::CommitToken;
use cheime_model::{Candidate, Key, KeyEvent};
use thiserror::Error;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipelineIntent {
    None,
    Cancel,
    CommitHighlighted,
    /// Commit the raw composition text without candidate conversion.
    CommitRaw,
    /// Commit a specific text directly (used by punctuator for single-commit symbols).
    CommitText(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelineUpdate {
    pub composition: String,
    pub candidates: Vec<ResolvedCandidate>,
    pub intent: PipelineIntent,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PipelineError {
    #[error("unsupported character {0:?}")]
    UnsupportedCharacter(char),
}

/// Top-level trait consumed by Session. All pipelines implement this.
pub trait InputPipeline: Send + Sync {
    fn apply(&self, composition: &str, event: &KeyEvent) -> Result<PipelineUpdate, PipelineError>;

    fn refresh(&self, _composition: &str) -> Result<Vec<ResolvedCandidate>, PipelineError> {
        Ok(Vec::new())
    }

    fn schema_id(&self) -> &str {
        "default"
    }

    fn commit_applied(&self, _token: CommitToken, _record: CommitRecord) {}

    fn rollback_learning(&self, _token: CommitToken) {}
}

// ── Component traits ────────────────────────────────────────────────

/// Result of processing a key event.
#[derive(Debug)]
pub struct ProcessorOutput {
    pub composition: String,
    pub intent: PipelineIntent,
    pub consumed: bool,
    /// Candidates injected by the processor (e.g. punctuator symbol expansion).
    /// Appended before translator candidates.
    pub inject_candidates: Vec<Candidate>,
}

pub trait Processor: Send {
    fn process(
        &mut self,
        composition: &str,
        event: &KeyEvent,
    ) -> Result<ProcessorOutput, PipelineError>;
}

// ── Segmentor ───────────────────────────────────────────────────────

/// A single code segment produced by a Segmentor.
#[derive(Clone, Debug)]
pub struct CodeSegment {
    /// The matched code string (e.g. "zhong").
    pub code: String,
    /// The segment label — "pinyin", "url", "number", etc.
    pub tag: String,
}

pub trait Segmentor: Send + Sync {
    fn segment(&self, composition: &str) -> SegmentationGraph;
}

// ── Translator ──────────────────────────────────────────────────────

pub trait Translator: Send + Sync {
    /// Return the translator's display name for diagnostics.
    fn name(&self) -> &str;

    /// Produce candidates for a sequence of code segments.
    /// Translators receive all segments at once, allowing multi-syllable
    /// dictionary lookups (e.g., segments ["ni", "hao"] → query "ni hao").
    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate>;

    fn translate_graph(&self, graph: &SegmentationGraph) -> Vec<ResolvedCandidate> {
        let path = graph.primary_path();
        let code = path
            .iter()
            .map(|segment| segment.code.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        self.translate(&path)
            .into_iter()
            .map(|candidate| ResolvedCandidate {
                display: candidate,
                consumed: crate::segmentation::InputSpan::new(0, graph.input_len()),
                canonical_code: code.clone(),
                lexemes: Vec::new(),
                complete: true,
                exact_phrase: false,
                completion: false,
                score: 0,
            })
            .collect()
    }
}

// ── Filter ──────────────────────────────────────────────────────────

pub trait Filter: Send + Sync {
    fn name(&self) -> &str;
    fn filter(&self, candidates: Vec<ResolvedCandidate>) -> Vec<ResolvedCandidate>;
}

// ── Ranker ──────────────────────────────────────────────────────────

pub trait Ranker: Send + Sync {
    fn name(&self) -> &str;
    fn rank(&self, candidates: Vec<ResolvedCandidate>) -> Vec<ResolvedCandidate>;
}

// ── ComposablePipeline ──────────────────────────────────────────────
pub struct ComposablePipeline {
    processor: parking_lot::Mutex<Box<dyn Processor>>,
    segmentor: Box<dyn Segmentor>,
    normalizer: Option<Box<dyn CodeNormalizer>>,
    translators: Vec<Box<dyn Translator>>,
    filters: Vec<Box<dyn Filter>>,
    ranker: Box<dyn Ranker>,
    key_mapper: Option<parking_lot::Mutex<Box<dyn KeyMapper>>>,
    learning: Option<std::sync::Arc<crate::learning::LearningService>>,
    schema_id: String,
}

impl ComposablePipeline {
    pub fn new(
        processor: Box<dyn Processor>,
        segmentor: Box<dyn Segmentor>,
        normalizer: Option<Box<dyn CodeNormalizer>>,
        translators: Vec<Box<dyn Translator>>,
        filters: Vec<Box<dyn Filter>>,
        ranker: Box<dyn Ranker>,
    ) -> Self {
        Self {
            processor: parking_lot::Mutex::new(processor),
            segmentor,
            normalizer,
            translators,
            filters,
            ranker,
            key_mapper: None,
            learning: None,
            schema_id: String::from("default"),
        }
    }

    pub fn with_key_mapper(mut self, km: Box<dyn KeyMapper>) -> Self {
        self.key_mapper = Some(parking_lot::Mutex::new(km));
        self
    }

    pub fn with_learning(
        mut self,
        learning: std::sync::Arc<crate::learning::LearningService>,
    ) -> Self {
        self.learning = Some(learning);
        self
    }

    pub fn with_schema_id(mut self, schema_id: impl Into<String>) -> Self {
        self.schema_id = schema_id.into();
        self
    }
}

impl InputPipeline for ComposablePipeline {
    fn apply(&self, composition: &str, event: &KeyEvent) -> Result<PipelineUpdate, PipelineError> {
        if let Some(km) = &self.key_mapper {
            let mut km = km.lock();
            let mapped = km.map(event);
            if mapped.consumed {
                return Ok(PipelineUpdate {
                    composition: composition.to_owned(),
                    candidates: vec![],
                    intent: PipelineIntent::None,
                });
            }
            let mut comp = composition.to_owned();
            let mut last = PipelineUpdate {
                composition: comp.clone(),
                candidates: vec![],
                intent: PipelineIntent::None,
            };
            for ch in &mapped.characters {
                let ke = KeyEvent {
                    key: Key::Character(*ch),
                    state: event.state,
                };
                last = self.apply_internal(&comp, &ke)?;
                comp = last.composition.clone();
            }
            return Ok(last);
        }
        self.apply_internal(composition, event)
    }

    fn refresh(&self, composition: &str) -> Result<Vec<ResolvedCandidate>, PipelineError> {
        Ok(self.resolve_composition(composition, Vec::new()))
    }

    fn schema_id(&self) -> &str {
        &self.schema_id
    }

    fn commit_applied(&self, token: CommitToken, record: CommitRecord) {
        if let Some(learning) = &self.learning {
            learning.commit_applied(token, record);
        }
    }

    fn rollback_learning(&self, token: CommitToken) {
        if let Some(learning) = &self.learning {
            learning.rollback_learning(token);
        }
    }
}
impl ComposablePipeline {
    fn resolve_composition(
        &self,
        composition: &str,
        injected: Vec<Candidate>,
    ) -> Vec<ResolvedCandidate> {
        let graph = self.segmentor.segment(composition);
        let normalized = if let Some(normalizer) = &self.normalizer {
            normalizer.normalize_graph(&graph)
        } else {
            graph
        };
        let path = normalized.primary_path();
        let code = path
            .iter()
            .map(|segment| segment.code.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut candidates: Vec<ResolvedCandidate> = injected
            .into_iter()
            .map(|candidate| ResolvedCandidate {
                consumed: crate::segmentation::InputSpan::new(0, normalized.input_len()),
                canonical_code: code.clone(),
                lexemes: vec![SelectedLexeme {
                    text: candidate.text.clone(),
                    canonical_code: code.clone(),
                    weight: 0,
                    source: candidate.source.clone(),
                }],
                display: candidate,
                complete: true,
                exact_phrase: true,
                completion: false,
                score: 0,
            })
            .collect();
        for translator in &self.translators {
            candidates.extend(translator.translate_graph(&normalized));
        }
        for filter in &self.filters {
            candidates = filter.filter(candidates);
        }
        candidates = self.ranker.rank(candidates);
        for (index, candidate) in candidates.iter_mut().enumerate() {
            candidate.display.id = CandidateId::new(index as u64 + 1);
        }
        candidates
    }

    fn apply_internal(
        &self,
        composition: &str,
        event: &KeyEvent,
    ) -> Result<PipelineUpdate, PipelineError> {
        let mut proc = self.processor.lock();
        let proc_out = proc.process(composition, event)?;
        if proc_out.consumed {
            return Ok(PipelineUpdate {
                composition: proc_out.composition,
                candidates: vec![],
                intent: proc_out.intent,
            });
        }
        let candidates =
            self.resolve_composition(&proc_out.composition, proc_out.inject_candidates);
        Ok(PipelineUpdate {
            composition: proc_out.composition,
            candidates,
            intent: proc_out.intent,
        })
    }
}
