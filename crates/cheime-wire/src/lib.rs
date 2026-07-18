#![forbid(unsafe_code)]

pub mod codec;
pub mod error;
pub mod frame;

pub use codec::MessageCodec;
pub use error::WireError;
pub use frame::{FramedReader, FramedWriter};
