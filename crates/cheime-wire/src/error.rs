use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WireError {
    #[error("message size {actual} exceeds limit {max}")]
    SizeExceeded { actual: usize, max: usize },

    #[error("encode error: {0}")]
    Encode(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("incomplete frame: expected {expected} bytes, available {available} bytes")]
    IncompleteFrame { expected: usize, available: usize },

    #[error("invalid frame length")]
    InvalidFrameLength,

    #[error("protocol violation: {0}")]
    ProtocolViolation(String),
}
