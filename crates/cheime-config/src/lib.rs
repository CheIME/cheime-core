#![forbid(unsafe_code)]

pub mod deploy;
pub mod error;
pub mod layer;
pub mod merge;
pub mod schema;

pub use deploy::{DeployError, DeploymentHandle, DeploymentManager};
pub use error::ConfigError;
pub use layer::ConfigLayer;
pub use merge::ConfigLoader;
pub use schema::{
    EngineConfig, MenuConfig, PunctuatorConfig, SchemaConfig, SpellerAlgebra, SpellerConfig,
    SwitchConfig, SwitchGroup,
};
