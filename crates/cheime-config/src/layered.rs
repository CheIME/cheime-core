//! Layered config resolution — 4-layer merge (DRAFT §config, §3.7 compat/native split).
//!
//! CheIME advantage: explicit layer separation prevents the program from
//! touching user config. Rime stores everything in the same directory,
//! making it easy for upgrades to overwrite user customizations.
//!
//! ## Layers (lowest to highest priority)
//!
//! 1. **System** — built-in defaults (hardcoded in binary)
//! 2. **Schema** — the deployed schema (managed by DeploymentManager)
//! 3. **Profile** — user preferences (managed by user/sync tools)
//! 4. **Session** — runtime state (managed by the engine itself)
//!
//! Higher layers override lower layers. Only layer 4 (Session)
//! is written by the engine. Layers 1-3 are read-only to the engine.
//!
//! ## Directory layout
//! ```text
//! {data_dir}/
//!   runtime/
//!     deployments/       ← layer 2: system-managed, read-only
//!     current.txt
//!   user/
//!     profile.yaml       ← layer 3: user-managed, read-only to engine
//!   state/
//!     session.json       ← layer 4: engine-managed, written by engine
//! ```

use crate::profile::UserProfile;
use crate::schema::SchemaConfig;
use crate::state::RuntimeState;
use std::path::PathBuf;

// ── LayeredConfig ──────────────────────────────────────────────────

/// Resolves a SchemaConfig from layered sources.
pub struct LayeredConfig {
    data_dir: PathBuf,
}

impl LayeredConfig {
    /// Create a resolver rooted at `data_dir`.
    /// See the module docs for expected directory layout.
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    // ── Path helpers ────────────────────────────────────────────────

    fn user_dir(&self) -> PathBuf {
        self.data_dir.join("user")
    }
    fn state_dir(&self) -> PathBuf {
        self.data_dir.join("state")
    }
    fn profile_path(&self) -> PathBuf {
        self.user_dir().join("profile.yaml")
    }
    fn session_path(&self) -> PathBuf {
        self.state_dir().join("session.json")
    }

    // ── Resolution ──────────────────────────────────────────────────

    /// Resolve the full layered config for a given deployed schema.
    ///
    /// 1. Load deployed schema (layer 2 — from DeploymentManager)
    /// 2. Load user profile (layer 3) and patch on top
    /// 3. Load runtime state (layer 4) and apply switch overrides
    ///
    /// The layer-1 system defaults are embedded in [`SchemaConfig::default()`].
    pub fn resolve(
        &self,
        deployed: &SchemaConfig,
    ) -> Result<LayeredSchema, crate::error::ConfigError> {
        // Layer 1: System defaults (SchemaConfig::default() → empty)
        let mut merged = SchemaConfig::default();

        // Layer 2: Deployed schema
        merged = crate::merge::merge_configs(merged, deployed.clone());

        // Layer 3: User profile (if exists)
        let profile = UserProfile::load(&self.profile_path())?;
        if let Some(patch) = &profile.patch {
            merged = crate::merge::merge_configs(merged, patch.clone());
        }

        // Layer 4: Runtime state — apply only state-tracked changes directly
        let state = RuntimeState::load(&self.session_path())?;
        apply_state_overrides(&mut merged, &state);
        let half_shape = state.switch("half_shape");

        Ok(LayeredSchema {
            config: merged,
            profile,
            state,
            half_shape,
        })
    }

    /// Save runtime state (call on shutdown or schema switch).
    pub fn save_state(&self, state: &RuntimeState) -> Result<(), crate::error::ConfigError> {
        state.save(&self.session_path())
    }

    /// Save user profile (called by settings UI or sync tools).
    pub fn save_profile(&self, profile: &UserProfile) -> Result<(), crate::error::ConfigError> {
        profile.save(&self.profile_path())
    }
}

// ── LayeredSchema ──────────────────────────────────────────────────

/// Fully resolved schema with layer provenance.
pub struct LayeredSchema {
    /// The final merged schema config.
    pub config: SchemaConfig,
    /// The user profile that was merged (for introspection).
    pub profile: UserProfile,
    /// The runtime state that was merged (for introspection).
    pub state: RuntimeState,
    /// Whether half_shape punctuator mode is active.
    pub half_shape: bool,
}

// ── Runtime state → config overrides ──────────────────────────────

/// Convert runtime switch state into a partial SchemaConfig overlay.
///
/// This is how session-level toggles feed into the config system
/// without the engine writing to user files.
fn apply_state_overrides(config: &mut SchemaConfig, state: &RuntimeState) {
    // Only apply changes that the session explicitly tracks.
    // Currently no SchemaConfig fields are mapped — switches affect
    // runtime behavior (half_shape, ascii_punct, simplification) at
    // PipelineFactory build time, not SchemaConfig values.
    let _ = state;
    let _ = config;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::DeploymentManager;
    use tempfile::TempDir;

    fn base_deployed_yaml() -> &'static str {
        r#"schema_version: 1
engine:
  processors:
    - type: ascii_composer
  segmentors:
    - type: pinyin_syllable
  translators:
    - type: dict
      dictionary: base
menu:
  page_size: 9
"#
    }

    #[test]
    fn layered_resolve_without_profile_or_state() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path();

        // Deploy a schema (layer 2)
        let mgr = DeploymentManager::new(data.join("runtime"));
        let deployed = mgr.deploy(base_deployed_yaml()).unwrap();

        let layered = LayeredConfig::new(data.to_path_buf());
        let resolved = layered.resolve(&deployed.schema).unwrap();

        assert_eq!(resolved.config.menu.page_size, 9);
        assert!(resolved.profile.schema.is_none()); // No user profile
        assert!(resolved.state.switches.is_empty()); // No runtime state
    }

    #[test]
    fn user_profile_overrides_deployed() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path();

        // Deploy a schema
        let mgr = DeploymentManager::new(data.join("runtime"));
        let deployed = mgr.deploy(base_deployed_yaml()).unwrap();

        // Write user profile with page_size override
        let user_dir = data.join("user");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(
            user_dir.join("profile.yaml"),
            "schema: quanpin\npatch:\n  schema_version: 1\n  menu:\n    page_size: 5\n  engine: {}\n",
        ).unwrap();

        let layered = LayeredConfig::new(data.to_path_buf());
        let resolved = layered.resolve(&deployed.schema).unwrap();

        // User's page_size wins over deployed
        let patch = resolved.profile.patch.as_ref().unwrap();
        assert_eq!(
            resolved.config.menu.page_size, 5,
            "user override should win (patch={}, resolved={}, deployed={})",
            patch.menu.page_size, resolved.config.menu.page_size, deployed.schema.menu.page_size
        );
    }

    #[test]
    fn profile_and_state_layers_stack() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path();
        let mgr = DeploymentManager::new(data.join("runtime"));
        let deployed = mgr.deploy(base_deployed_yaml()).unwrap();

        // Layer 3: user profile sets page_size=5
        let user_dir = data.join("user");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(
            user_dir.join("profile.yaml"),
            r#"schema: quanpin
patch:
  schema_version: 1
  menu:
    page_size: 5
  engine: {}
"#,
        )
        .unwrap();

        // Layer 4: runtime state
        let state_dir = data.join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let mut state = RuntimeState::default();
        state.active_schema = Some("quanpin".into());
        state.set_switch("half_shape", true);
        state.save(&state_dir.join("session.json")).unwrap();

        let layered = LayeredConfig::new(data.to_path_buf());
        let resolved = layered.resolve(&deployed.schema).unwrap();

        // Layer 3 override wins
        assert_eq!(resolved.config.menu.page_size, 5);
        // Layer 4 state preserved
        assert!(resolved.half_shape);
        assert_eq!(resolved.state.active_schema.as_deref(), Some("quanpin"));
    }
}
