#![forbid(unsafe_code)]

mod builtin;
pub mod factory;
pub mod filter;
pub mod key_mapper;
pub mod normalizer;
pub mod processor;
pub mod ranker;
pub mod segmentor;
pub mod translator;
use cheime_model::{Candidate, KeyEvent};
use crate::normalizer::CodeNormalizer;
use thiserror::Error;

pub use builtin::BuiltinPipeline;

// ── Composable pipeline traits ──────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipelineIntent {
    None,
    Cancel,
    CommitHighlighted,
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
#[derive(Clone, Debug)]
pub struct ProcessorOutput {
    /// The new composition text after processing.
    pub composition: String,
    /// What the user intends (cancel / commit).
    pub intent: PipelineIntent,
    /// True if the event was fully consumed — skip segment/translate/rank.
    pub consumed: bool,
}

pub trait Processor: Send + Sync {
    fn process(
        &self,
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

    /// Produce candidates for a single code segment.
    fn translate(&self, segment: &CodeSegment) -> Vec<Candidate>;
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
    processor: Box<dyn Processor>,
    segmentor: Box<dyn Segmentor>,
    normalizer: Option<Box<dyn CodeNormalizer>>,
    translators: Vec<Box<dyn Translator>>,
    filters: Vec<Box<dyn Filter>>,
    ranker: Box<dyn Ranker>,
}

impl ComposablePipeline {
    pub fn new(
        processor: Box<dyn Processor>, segmentor: Box<dyn Segmentor>,
        normalizer: Option<Box<dyn CodeNormalizer>>,
        translators: Vec<Box<dyn Translator>>, filters: Vec<Box<dyn Filter>>,
        ranker: Box<dyn Ranker>,
    ) -> Self {
        Self { processor, segmentor, normalizer, translators, filters, ranker }
    }
}

impl InputPipeline for ComposablePipeline {
    fn apply(
        &self,
        composition: &str,
        event: &KeyEvent,
    ) -> Result<PipelineUpdate, PipelineError> {
        // 1. Processor — key handling
        let proc = self.processor.process(composition, event)?;

        if proc.consumed {
            // Action handled entirely by processor (e.g. Escape cancel).
            return Ok(PipelineUpdate {
                composition: proc.composition,
                candidates: Vec::new(),
                intent: proc.intent,
            });
        }

        // 2. Segmentor — split composition into code units
        let segments = self.segmentor.segment(&proc.composition);

        // 2.5 Normalizer — expand segments into fuzzy/spelling variants
        let variants: Vec<CodeSegment> = if let Some(n) = &self.normalizer {
            segments.iter().flat_map(|s| n.normalize(s)).collect()
        } else {
            segments
        };

        // 3. Translators — collect candidates per variant
        let mut candidates: Vec<Candidate> = Vec::new();
        for seg in &variants {
            for translator in &self.translators {
                candidates.extend(translator.translate(seg));
            }
        }

        // 5. Ranker — sort
        candidates = self.ranker.rank(candidates);

        Ok(PipelineUpdate {
            composition: proc.composition,
            candidates,
            intent: proc.intent,
        })
    }
}
