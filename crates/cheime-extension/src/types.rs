use cheime_model::{Candidate, KeyEvent, Revision, SessionEpoch};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    pub tag: String,
}

#[derive(Clone, Debug)]
pub struct ExtensionCandidate {
    pub text: String,
    pub code: String,
    pub weight: Option<i64>,
    pub annotation: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExtensionContext<'a> {
    pub session_epoch: SessionEpoch,
    pub revision: Revision,
    pub composition: &'a str,
    pub schema_id: &'a str,
}

#[derive(Clone, Debug)]
pub enum ExtensionOutput {
    Processor { handled: bool, composition: String },
    Segmentor { segments: Vec<Segment> },
    Translator { candidates: Vec<ExtensionCandidate> },
    Filter { candidates: Vec<Candidate> },
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ExtensionError {
    #[error("extension error: {0}")]
    Runtime(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("execution timed out")]
    Timeout,
    #[error("extension not found: {0}")]
    NotFound(String),
}

pub trait Processor: Send + Sync {
    fn process(
        &self,
        ctx: &ExtensionContext,
        key: &KeyEvent,
    ) -> Result<ExtensionOutput, ExtensionError>;
}

pub trait Segmentor: Send + Sync {
    fn segment(&self, ctx: &ExtensionContext) -> Result<ExtensionOutput, ExtensionError>;
}

pub trait Translator: Send + Sync {
    fn translate(&self, ctx: &ExtensionContext) -> Result<ExtensionOutput, ExtensionError>;
}

pub trait Filter: Send + Sync {
    fn filter(
        &self,
        ctx: &ExtensionContext,
        candidates: &[Candidate],
    ) -> Result<ExtensionOutput, ExtensionError>;
}

pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    fn processor(&self) -> Option<&dyn Processor> {
        None
    }
    fn segmentor(&self) -> Option<&dyn Segmentor> {
        None
    }
    fn translator(&self) -> Option<&dyn Translator> {
        None
    }
    fn filter(&self) -> Option<&dyn Filter> {
        None
    }
}
