#![forbid(unsafe_code)]

mod candidate;
mod ids;
mod input;

pub use candidate::{Candidate, CandidateSnapshot, SessionStatus};
pub use ids::{
    ActionId, CandidateId, ClientInstanceId, DeploymentGeneration, Revision, Sequence,
    SessionEpoch, SessionId,
};
pub use input::{
    CommitToken, Key, KeyEvent, KeyState, PlatformAction, PlatformActionKind,
    PlatformActionOutcome, PlatformActionResult, UiCommand,
};

/// Protocol version implemented by this core build.
pub const CORE_PROTOCOL_VERSION: u16 = 1;
#[cfg(test)]
mod serde_tests;
