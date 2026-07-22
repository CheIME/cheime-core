//! User profile config — patches the deployed schema with user preferences.
//!
//! Separated from deployed/system config (DRAFT §config).
//! The program NEVER writes to user/ — only the user or sync tools do.
//!
//! Location: `{data_dir}/user/profile.yaml`

use crate::schema::SchemaConfig;
use serde::{Deserialize, Serialize};

/// A user's personal preference layer.
///
/// This is a SPARSE overlay — only fields the user explicitly sets
/// are present. During merge, non-default values override the base.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserProfile {
    /// Which schema this profile patches.
    /// e.g. "quanpin" or "flypy" — matches a schema file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Sparse patch overlay. Only set the fields you want to change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<SchemaConfig>,
}

impl UserProfile {
    /// Load from a YAML file, or return default if missing.
    pub fn load(path: &std::path::Path) -> Result<Self, crate::error::ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::ConfigError::Io(e.to_string()))?;
        serde_yaml::from_str(&content).map_err(|e| crate::error::ConfigError::Parse {
            path: path.to_string_lossy().to_string(),
            message: e.to_string(),
        })
    }

    /// Save to a YAML file (used by sync tools and settings UI, NOT by the engine).
    pub fn save(&self, path: &std::path::Path) -> Result<(), crate::error::ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::ConfigError::Io(e.to_string()))?;
        }
        let yaml = serde_yaml::to_string(self).map_err(|e| crate::error::ConfigError::Parse {
            path: path.to_string_lossy().to_string(),
            message: e.to_string(),
        })?;
        std::fs::write(path, yaml).map_err(|e| crate::error::ConfigError::Io(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_file_returns_default() {
        let profile = UserProfile::load(std::path::Path::new("/nonexistent/profile.yaml")).unwrap();
        assert!(profile.schema.is_none());
        assert!(profile.patch.is_none());
    }

    #[test]
    fn round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("profile.yaml");

        let p = UserProfile {
            schema: Some("quanpin".into()),
            patch: Some(
                serde_yaml::from_str("schema_version: 1\nmenu:\n  page_size: 5\nengine: {}\n")
                    .unwrap(),
            ),
        };
        p.save(&path).unwrap();

        let loaded = UserProfile::load(&path).unwrap();
        assert_eq!(loaded.schema.as_deref(), Some("quanpin"));
        assert_eq!(loaded.patch.as_ref().unwrap().menu.page_size, 5);
    }
}
