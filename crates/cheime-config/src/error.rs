use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(String),

    #[error("YAML parse error at {path}: {message}")]
    Parse { path: String, message: String },

    #[error("unknown field '{field}' at {path}")]
    UnknownField { path: String, field: String },

    #[error("invalid value at {path}: expected {expected}, got {got}")]
    InvalidValue {
        path: String,
        expected: String,
        got: String,
    },

    #[error("circular dependency detected in extends chain: {0}")]
    CircularExtends(String),

    #[error("referenced config not found: {0}")]
    NotFound(String),

    #[error("unsupported schema version {version} (engine supports up to {max})")]
    UnsupportedVersion { version: u32, max: u32 },

    #[error("validation error at {path}: {message}")]
    Validation { path: String, message: String },
}

impl ConfigError {
    pub fn path(&self) -> Option<&str> {
        match self {
            ConfigError::Parse { path, .. }
            | ConfigError::UnknownField { path, .. }
            | ConfigError::InvalidValue { path, .. }
            | ConfigError::Validation { path, .. } => Some(path),
            _ => None,
        }
    }
}
