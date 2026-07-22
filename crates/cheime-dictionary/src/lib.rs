#![allow(clippy::clone_on_ref_ptr, clippy::needless_lifetimes)]
#![forbid(unsafe_code)]

pub mod body;
pub mod cache;
pub mod deploy;
pub mod header;
pub mod import;
pub mod index;
pub mod tiered;
pub use body::{BodyError, DictEntry, parse_body};
pub use cache::{CacheError, DictCache};
pub use deploy::{DeployError, DeploymentHandle, DeploymentManager};
pub use header::{DictColumn, DictHeader, HeaderError, parse_header};
pub use import::{ImportError, resolve_imports};
pub use index::CompiledIndex;

pub use index::MemoryIndex;
