#![allow(clippy::field_reassign_with_default)]
#![forbid(unsafe_code)]

pub mod deploy;
pub mod error;
pub mod layer;
pub mod layered;
pub mod merge;
pub mod profile;
pub mod schema;
pub mod state;

pub use deploy::{DeployError, DeploymentHandle, DeploymentManager};
pub use error::ConfigError;
pub use layer::ConfigLayer;
pub use layered::{LayeredConfig, LayeredSchema};
pub use merge::ConfigLoader;
pub use profile::UserProfile;
pub use schema::{
    AbbreviationConfig, EngineConfig, FuzzyPinyinConfig, MenuConfig, PunctuatorConfig,
    SchemaConfig, SpellerAlgebra, SpellerConfig, SwitchConfig, SwitchGroup,
};
pub use state::RuntimeState;
