#![forbid(unsafe_code)]

pub mod codec;
pub mod error;

pub use codec::MessageCodec;
pub use error::WireError;
