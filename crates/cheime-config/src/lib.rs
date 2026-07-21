#![forbid(unsafe_code)]

pub mod error;
pub mod layer;
pub mod schema;

pub use error::ConfigError;
pub use layer::ConfigLayer;
pub use schema::{
    EngineConfig, MenuConfig, SchemaConfig, SpellerAlgebra, SpellerConfig, SwitchConfig,
    SwitchGroup,
};
