//! Atomic deployment manager for typed schema configs (DRAFT §8.4-8.5).
//!
//! CheIME advantage: config deployment is versioned, validated, and
//! atomically switched. Rime edits configs in-place with no rollback.
//!
//! Deployments are stored as:
//! ```text
//! runtime/
//!   deployments/
//!     <timestamp>-<hash>/
//!       schema.yaml          ← the deployed schema config
//!       diagnostics.json     ← validation report
//!   current.txt             ← path of active deployment, e.g. "deployments/2026-07-21T08-00-00Z-a1b2c3"
//! ```
//!
//! On Windows, `current.txt` is used instead of a symlink (no admin required).
//! On Unix, a symlink is preferred when possible.

use crate::schema::SchemaConfig;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Errors ─────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum DeployError {
    Io(String),
    Parse(String),
    Validation(Vec<String>),
    MissingCurrent,
}

impl std::fmt::Display for DeployError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Parse(e) => write!(f, "parse: {e}"),
            Self::Validation(msgs) => {
                write!(f, "validation errors:")?;
                for m in msgs { write!(f, "\n  - {m}")?; }
                Ok(())
            }
            Self::MissingCurrent => write!(f, "no current deployment"),
        }
    }
}

// ── Deployment handle ──────────────────────────────────────────────

/// Read-only handle to the currently-active deployment.
#[derive(Clone, Debug)]
pub struct DeploymentHandle {
    /// Path to the deployment directory.
    dir: PathBuf,
    /// The parsed schema config from this deployment.
    pub schema: SchemaConfig,
    /// Timestamp of deployment.
    pub deployed_at: String,
    /// Content hash for verification.
    pub content_hash: String,
}

impl DeploymentHandle {
    pub fn dir(&self) -> &Path { &self.dir }
}

// ── Manager ────────────────────────────────────────────────────────

pub struct DeploymentManager {
    /// Root runtime directory (contains deployments/ and current.txt).
    runtime_dir: PathBuf,
}

impl DeploymentManager {
    pub fn new(runtime_dir: PathBuf) -> Self { Self { runtime_dir } }

    /// Returns the directory where deployment subdirectories live.
    fn deployments_dir(&self) -> PathBuf { self.runtime_dir.join("deployments") }

    /// Returns the path to the current-bootstrap file.
    fn current_file(&self) -> PathBuf { self.runtime_dir.join("current.txt") }

    // ── Deploy ─────────────────────────────────────────────────────

    /// Validate and deploy a new schema config. On success, the new deployment
    /// becomes the active one. On failure, the current deployment is untouched.
    pub fn deploy(&self, yaml: &str) -> Result<DeploymentHandle, DeployError> {
        // 1. Parse
        let schema: SchemaConfig = serde_yaml::from_str(yaml)
            .map_err(|e| DeployError::Parse(e.to_string()))?;

        // 2. Validate
        let errors = Self::validate(&schema);
        if !errors.is_empty() {
            return Err(DeployError::Validation(errors));
        }

        // 3. Create deployment directory
        let timestamp = Self::now_iso();
        let hash = Self::short_hash(yaml);
        let dir_name = format!("{timestamp}-{hash}");
        let deploy_dir = self.deployments_dir().join(&dir_name);
        std::fs::create_dir_all(&deploy_dir)
            .map_err(|e| DeployError::Io(format!("create dir: {e}")))?;

        // 4. Write schema.yaml
        let schema_path = deploy_dir.join("schema.yaml");
        std::fs::write(&schema_path, yaml)
            .map_err(|e| DeployError::Io(format!("write schema: {e}")))?;

        // 5. Write diagnostics.json
        Self::write_diagnostics(&deploy_dir, &schema, &errors)?;

        // 6. Atomic switch: write current.txt pointing to new deployment
        let relative = format!("deployments/{}", dir_name);
        // Write to temp file first, then rename (atomic on same fs)
        let tmp = self.runtime_dir.join("current.tmp");
        {
            let mut f = std::fs::File::create(&tmp)
                .map_err(|e| DeployError::Io(format!("create tmp: {e}")))?;
            write!(f, "{relative}\n")
                .map_err(|e| DeployError::Io(format!("write tmp: {e}")))?;
            f.flush().map_err(|e| DeployError::Io(format!("flush: {e}")))?;
        }
        std::fs::rename(&tmp, self.current_file())
            .map_err(|e| DeployError::Io(format!("atomic swap: {e}")))?;

        // 7. Return handle
        Ok(DeploymentHandle {
            dir: deploy_dir,
            schema,
            deployed_at: timestamp,
            content_hash: hash,
        })
    }

    // ── Current ─────────────────────────────────────────────────────

    /// Read the currently-active deployment.
    pub fn current(&self) -> Result<DeploymentHandle, DeployError> {
        let current_path = self.current_file();
        let content = std::fs::read_to_string(&current_path)
            .map_err(|_| DeployError::MissingCurrent)?;
        let relative = content.trim();
        let deploy_dir = self.runtime_dir.join(relative);
        let schema_path = deploy_dir.join("schema.yaml");
        let yaml = std::fs::read_to_string(&schema_path)
            .map_err(|e| DeployError::Io(format!("read schema: {e}")))?;
        let schema: SchemaConfig = serde_yaml::from_str(&yaml)
            .map_err(|e| DeployError::Parse(e.to_string()))?;
        let hash = Self::short_hash(&yaml);
        // Parse timestamp from dir name
        let dir_name = deploy_dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let deployed_at = dir_name.split('-').take(2).collect::<Vec<_>>().join("-");
        Ok(DeploymentHandle { dir: deploy_dir, schema, deployed_at, content_hash: hash })
    }

    /// List all deployed versions (newest first).
    pub fn list_deployments(&self) -> Result<Vec<String>, DeployError> {
        let d = self.deployments_dir();
        if !d.exists() {
            return Ok(vec![]);
        }
        let mut names: Vec<String> = std::fs::read_dir(&d)
            .map_err(|e| DeployError::Io(e.to_string()))?
            .filter_map(|e| {
                let e = e.ok()?;
                if e.file_type().ok()?.is_dir() {
                    e.file_name().into_string().ok()
                } else { None }
            })
            .collect();
        names.sort_by(|a, b| b.cmp(a)); // newest first
        Ok(names)
    }

    // ── Internal helpers ────────────────────────────────────────────

    fn validate(schema: &SchemaConfig) -> Vec<String> {
        let mut errors = Vec::new();
        // Basic structural validation
        if schema.schema_version == 0 {
            errors.push("schema_version must be >= 1".into());
        }
        // Menu sanity
        if schema.menu.page_size == 0 {
            errors.push("menu.page_size must be >= 1".into());
        }
        errors
    }

    fn write_diagnostics(dir: &Path, schema: &SchemaConfig, errors: &[String]) -> Result<(), DeployError> {
        let diag = serde_json::json!({
            "schema_version": schema.schema_version,
            "validation_errors": errors,
            "engine_summary": {
                "processors": schema.engine.processors.len(),
                "segmentors": schema.engine.segmentors.len(),
                "translators": schema.engine.translators.len(),
                "filters": schema.engine.filters.len(),
            },
        });
        let path = dir.join("diagnostics.json");
        let json = serde_json::to_string_pretty(&diag)
            .map_err(|e| DeployError::Parse(e.to_string()))?;
        std::fs::write(&path, json)
            .map_err(|e| DeployError::Io(format!("write diagnostics: {e}")))?;
        Ok(())
    }

    fn now_iso() -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Simple ISO-like: 2026-07-21T08-30-00Z (colons not allowed in Windows paths)
        let secs = ts % 60; let mins = (ts / 60) % 60; let hours = (ts / 3600) % 24;
        let days = ts / 86400;
        // Approximate date from epoch — good enough for deployment tagging
        format!("EPOCH-{days:05}-{hours:02}-{mins:02}-{secs:02}")
    }

    fn short_hash(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        format!("{:x}", hasher.finalize())
            .chars().take(8).collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn base_config() -> &'static str {
        r#"schema_version: 1
engine:
  processors:
    - type: ascii_composer
  segmentors:
    - type: pinyin_syllable
  translators:
    - type: dict
      dictionary: base
  filters:
    - type: uniquifier
menu:
  page_size: 5
"#
    }

    #[test]
    fn deploy_and_read_current() {
        let tmp = TempDir::new().unwrap();
        let mgr = DeploymentManager::new(tmp.path().to_path_buf());

        let handle = mgr.deploy(base_config()).unwrap();
        assert_eq!(handle.schema.menu.page_size, 5);
        assert!(handle.dir.exists());

        let current = mgr.current().unwrap();
        assert_eq!(current.schema.menu.page_size, 5);
        assert_eq!(current.content_hash, handle.content_hash);
    }

    #[test]
    fn atomic_switch_preserves_old() {
        let tmp = TempDir::new().unwrap();
        let mgr = DeploymentManager::new(tmp.path().to_path_buf());

        let h1 = mgr.deploy(base_config()).unwrap();

        let updated = base_config().replace("page_size: 5", "page_size: 9");
        let h2 = mgr.deploy(&updated).unwrap();

        assert_eq!(h2.schema.menu.page_size, 9);
        let current = mgr.current().unwrap();
        assert_eq!(current.schema.menu.page_size, 9);

        // Old deployment directory still exists
        assert!(h1.dir.exists());
        // List shows both
        let list = mgr.list_deployments().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn validation_error_rejects_deployment() {
        let tmp = TempDir::new().unwrap();
        let mgr = DeploymentManager::new(tmp.path().to_path_buf());

        let bad = r#"schema_version: 0
engine: {}
menu:
  page_size: 0
"#;
        let result = mgr.deploy(bad);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("schema_version"), "error: {err}");
        assert!(err.contains("page_size"), "error: {err}");

        // Current deployment is untouched (none exists)
        assert!(mgr.current().is_err());
    }

    #[test]
    fn parse_error_is_not_deployed() {
        let tmp = TempDir::new().unwrap();
        let mgr = DeploymentManager::new(tmp.path().to_path_buf());

        let result = mgr.deploy("not valid yaml: ::");
        assert!(result.is_err());
        // No deployments directory created
        assert!(!mgr.deployments_dir().exists() || mgr.list_deployments().unwrap().is_empty());
    }

    #[test]
    fn no_current_returns_error() {
        let tmp = TempDir::new().unwrap();
        let mgr = DeploymentManager::new(tmp.path().to_path_buf());
        assert!(mgr.current().is_err());
    }

    #[test]
    fn diagnostics_file_written() {
        let tmp = TempDir::new().unwrap();
        let mgr = DeploymentManager::new(tmp.path().to_path_buf());

        let handle = mgr.deploy(base_config()).unwrap();
        let diag_path = handle.dir.join("diagnostics.json");
        assert!(diag_path.exists());
        let diag: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&diag_path).unwrap()).unwrap();
        assert_eq!(diag["schema_version"], 1);
    }

    #[test]
    fn base_schema_loads_and_validates() {
        let base_yaml = include_str!("../../../config/schemas/base.yaml");
        let schema: crate::schema::SchemaConfig = serde_yaml::from_str(base_yaml).unwrap();
        assert_eq!(schema.schema_version, 1);
        assert_eq!(schema.engine.segmentors.len(), 1);
        assert_eq!(schema.menu.page_size, 9);
    }

    #[test]
    fn quanpin_extends_base() {
        let quanpin_yaml = include_str!("../../../config/schemas/quanpin.yaml");
        let quanpin: crate::schema::SchemaConfig = serde_yaml::from_str(quanpin_yaml).unwrap();
        assert_eq!(quanpin.extends.len(), 1);
        assert!(quanpin.speller.is_some());
        let speller = quanpin.speller.as_ref().unwrap();
        assert_eq!(speller.initials.as_deref(), Some("bpmfdtnlgkhjqxzcsryw"));
    }
}
