#![allow(clippy::needless_range_loop)]
#![forbid(unsafe_code)]

mod builtin;
pub mod emoji;
pub mod factory;
pub mod filter;
pub mod key_mapper;
pub mod normalizer;
pub mod processor;
pub mod punctuator;
pub mod ranker;
pub mod segmentor;
pub mod simplifier;
pub mod translator;
use crate::key_mapper::KeyMapper;
use crate::normalizer::CodeNormalizer;
pub use builtin::BuiltinPipeline;
use cheime_model::{Candidate, CandidateId, Key, KeyEvent};
use thiserror::Error;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipelineIntent {
    None,
    Cancel,
    CommitHighlighted,
    /// Commit the raw composition text as-is (Enter key / predict mode).
    CommitRaw,
    /// Commit a specific text directly (used by punctuator for single-commit symbols).
    CommitText(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelineUpdate {
    pub composition: String,
    pub candidates: Vec<Candidate>,
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
    fn segment(&self, composition: &str) -> Vec<CodeSegment>;
}

// ── Translator ──────────────────────────────────────────────────────

pub trait Translator: Send + Sync {
    /// Return the translator's display name for diagnostics.
    fn name(&self) -> &str;

    /// Produce candidates for a sequence of code segments.
    /// Translators receive all segments at once, allowing multi-syllable
    /// dictionary lookups (e.g., segments ["ni", "hao"] → query "ni hao").
    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate>;
}

// ── Filter ──────────────────────────────────────────────────────────

pub trait Filter: Send + Sync {
    fn name(&self) -> &str;
    fn filter(&self, candidates: Vec<Candidate>) -> Vec<Candidate>;
}

// ── Ranker ──────────────────────────────────────────────────────────

pub trait Ranker: Send + Sync {
    fn name(&self) -> &str;
    fn rank(&self, candidates: Vec<Candidate>) -> Vec<Candidate>;
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
        }
    }

    pub fn with_key_mapper(mut self, km: Box<dyn KeyMapper>) -> Self {
        self.key_mapper = Some(parking_lot::Mutex::new(km));
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
}
impl ComposablePipeline {
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
        let segments = self.segmentor.segment(&proc_out.composition);
        let variants: Vec<CodeSegment> = if let Some(n) = &self.normalizer {
            n.normalize_all(&segments)
        } else {
            segments
        };
        let mut candidates = proc_out.inject_candidates;
        for t in &self.translators {
            candidates.extend(t.translate(&variants));
        }
        for f in &self.filters {
            candidates = f.filter(candidates);
        }
        candidates = self.ranker.rank(candidates);
        for (index, candidate) in candidates.iter_mut().enumerate() {
            candidate.id = CandidateId::new(index as u64 + 1);
        }
        Ok(PipelineUpdate {
            composition: proc_out.composition,
            candidates,
            intent: proc_out.intent,
        })
    }
}
