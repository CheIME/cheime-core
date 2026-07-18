#![forbid(unsafe_code)]

mod builtin;

use cheime_model::{Candidate, KeyEvent};
use thiserror::Error;

pub use builtin::BuiltinPipeline;

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

pub trait InputPipeline: Send + Sync {
    fn apply(&self, composition: &str, event: &KeyEvent) -> Result<PipelineUpdate, PipelineError>;
}
