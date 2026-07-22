//! Runtime state — program-maintained session/persistence state.
//!
//! The ENGINE writes to this file. The USER does not. This keeps
//! user config clean and lets the program track its own switches,
//! last-used schema, and other transient state between restarts.
//!
//! Location: `{data_dir}/state/session.json`

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Program-maintained runtime state.
///
/// Written by the engine on shutdown / schema switch.
/// Read on startup to restore previous session.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RuntimeState {
    /// Which schema was last active (e.g. "quanpin" or "flypy").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_schema: Option<String>,

    /// Switch toggles — persistent user preferences managed by the engine.
    /// e.g. "half_shape": true, "ascii_punct": false, "simplification": true
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub switches: BTreeMap<String, bool>,

    /// Last session timestamp (Unix seconds).
    #[serde(default)]
    pub last_active: u64,
}

impl RuntimeState {
    /// Load from a JSON file, or return default if missing.
    pub fn load(path: &std::path::Path) -> Result<Self, crate::error::ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::ConfigError::Io(e.to_string()))?;
        serde_json::from_str(&content).map_err(|e| crate::error::ConfigError::Parse {
            path: path.to_string_lossy().to_string(),
            message: e.to_string(),
        })
    }

    /// Save to a JSON file (called on shutdown / schema switch).
    pub fn save(&self, path: &std::path::Path) -> Result<(), crate::error::ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::ConfigError::Io(e.to_string()))?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| crate::error::ConfigError::Parse {
                path: path.to_string_lossy().to_string(),
                message: e.to_string(),
            })?;
        std::fs::write(path, json).map_err(|e| crate::error::ConfigError::Io(e.to_string()))?;
        Ok(())
    }

    /// Get a switch value, defaulting to `false`.
    pub fn switch(&self, name: &str) -> bool {
        self.switches.get(name).copied().unwrap_or(false)
    }

    /// Toggle a switch.
    pub fn set_switch(&mut self, name: impl Into<String>, value: bool) {
        self.switches.insert(name.into(), value);
    }

    /// Update timestamp to now.
    pub fn touch(&mut self) {
        self.last_active = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_returns_default() {
        let state = RuntimeState::load(std::path::Path::new("/nonexistent/session.json")).unwrap();
        assert!(state.active_schema.is_none());
        assert!(state.switches.is_empty());
    }

    #[test]
    fn round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("session.json");

        let mut s = RuntimeState::default();
        s.active_schema = Some("flypy".into());
        s.set_switch("half_shape", true);
        s.set_switch("ascii_punct", false);
        s.touch();
        s.save(&path).unwrap();

        let loaded = RuntimeState::load(&path).unwrap();
        assert_eq!(loaded.active_schema.as_deref(), Some("flypy"));
        assert!(loaded.switch("half_shape"));
        assert!(!loaded.switch("ascii_punct"));
        assert!(loaded.last_active > 0);
    }

    #[test]
    fn switch_defaults_to_false() {
        let state = RuntimeState::default();
        assert!(!state.switch("nonexistent"));
    }
}
