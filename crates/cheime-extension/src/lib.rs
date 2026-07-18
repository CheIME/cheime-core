#![forbid(unsafe_code)]

pub mod host;
pub mod types;

pub use host::ExtensionHost;
pub use types::{
    Extension, ExtensionCandidate, ExtensionContext, ExtensionError, ExtensionOutput, Filter,
    Processor, Segment, Segmentor, Translator,
};
