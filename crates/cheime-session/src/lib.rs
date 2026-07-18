#![forbid(unsafe_code)]

mod state;

pub use state::Session;

use cheime_model::{ActionId, Revision, Sequence, SessionEpoch};
use cheime_pipeline::PipelineError;
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SessionError {
    #[error("message epoch {received:?} does not match session epoch {expected:?}")]
    StaleEpoch {
        received: SessionEpoch,
        expected: SessionEpoch,
    },
    #[error("message sequence {received:?} is not newer than {last:?}")]
    StaleSequence { received: Sequence, last: Sequence },
    #[error("message revision {received:?} does not match current revision {current:?}")]
    StaleRevision {
        received: Revision,
        current: Revision,
    },
    #[error("revision overflow")]
    RevisionOverflow,
    #[error("no candidate is available to commit")]
    NoCandidate,
    #[error("platform action {0:?} is not pending")]
    UnknownAction(ActionId),
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
}
