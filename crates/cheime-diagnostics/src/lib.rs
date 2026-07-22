//! Structured diagnostic errors (DRAFT §16).
//!
//! CheIME advantage: every error carries a stable error code, structured
//! metadata, and a fix suggestion. Rime errors are ad-hoc strings with
//! no standard format or programmatic consumption path.
//!
//! Error codes follow the pattern:
//!   E-{DOMAIN}-{KIND}
//!
//! Examples:
//!   E-CONFIG-VALIDATION    — config validation failure
//!   E-RIME-UNSUPPORTED     — unsupported Rime component
//!   E-COMPONENT-BUILD      — pipeline component build failure
//!   E-SESSION-STALE        — stale message rejected
//!   E-DICT-IMPORT          — dictionary import failure
//!   E-DEPLOY-IO            — deployment I/O error
//!   E-PIPELINE-INTEGRITY   — pipeline invariant violation

use serde::{Deserialize, Serialize};

// ── Severity ───────────────────────────────────────────────────────

/// Error severity per DRAFT §16.2.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Config deployment: reject new version, keep current running.
    ConfigDeploy,
    /// Component init: disable that schema, others continue.
    ComponentInit,
    /// Script error: disable that instance, report.
    Script,
    /// Network error: provider temporarily unavailable, local continues.
    Network,
    /// Data corruption: switch to read-only or restore snapshot.
    DataCorruption,
    /// Platform error: close session and report.
    Platform,
    /// Engine invariant: terminate process, generate crash report.
    Fatal,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigDeploy => write!(f, "config_deploy"),
            Self::ComponentInit => write!(f, "component_init"),
            Self::Script => write!(f, "script"),
            Self::Network => write!(f, "network"),
            Self::DataCorruption => write!(f, "data_corruption"),
            Self::Platform => write!(f, "platform"),
            Self::Fatal => write!(f, "fatal"),
        }
    }
}

// ── DiagnosticError ────────────────────────────────────────────────

/// A structured diagnostic error (DRAFT §16.1).
///
/// Every field is optional except `code`, `severity`, and `message`.
/// Omitted fields are simply absent from the serialized output.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiagnosticError {
    /// Stable error code, e.g. `E-CONFIG-VALIDATION`.
    pub code: String,

    /// Error severity.
    pub severity: Severity,

    /// Human-readable user-facing description.
    pub message: String,

    /// Technical/internal reason (for logs, may be elided in UI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_reason: Option<String>,

    /// Path to the file that caused the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,

    /// JSON-pointer-style path within the config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,

    /// Schema name (e.g. `quanpin.schema.yaml`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Component type or instance name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,

    /// Platform affected (e.g. `windows`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    /// Capability that was requested but unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,

    /// Suggested fix or next step for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_suggestion: Option<String>,

    /// Correlated log identifier for traceability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_id: Option<String>,
}

impl DiagnosticError {
    pub fn new(code: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            technical_reason: None,
            file: None,
            config_path: None,
            schema: None,
            component: None,
            platform: None,
            capability: None,
            fix_suggestion: None,
            log_id: None,
        }
    }

    // ── Builder methods ────────────────────────────────────────────

    pub fn with_technical(mut self, reason: impl Into<String>) -> Self {
        self.technical_reason = Some(reason.into());
        self
    }
    pub fn with_file(mut self, f: impl Into<String>) -> Self {
        self.file = Some(f.into());
        self
    }
    pub fn with_config_path(mut self, p: impl Into<String>) -> Self {
        self.config_path = Some(p.into());
        self
    }
    pub fn with_schema(mut self, s: impl Into<String>) -> Self {
        self.schema = Some(s.into());
        self
    }
    pub fn with_component(mut self, c: impl Into<String>) -> Self {
        self.component = Some(c.into());
        self
    }
    pub fn with_fix(mut self, s: impl Into<String>) -> Self {
        self.fix_suggestion = Some(s.into());
        self
    }
    pub fn with_log_id(mut self, id: impl Into<String>) -> Self {
        self.log_id = Some(id.into());
        self
    }

    // ── Display ────────────────────────────────────────────────────

    /// Human-readable one-liner: `[E-CONFIG] message`.
    pub fn summary(&self) -> String {
        format!("[{}] {}", self.code, self.message)
    }

    /// Detailed multi-line report.
    pub fn detailed(&self) -> String {
        let mut lines = vec![
            format!("code: {}", self.code),
            format!("severity: {}", self.severity),
            format!("message: {}", self.message),
        ];
        if let Some(ref r) = self.technical_reason {
            lines.push(format!("reason: {r}"));
        }
        if let Some(ref f) = self.file {
            lines.push(format!("file: {f}"));
        }
        if let Some(ref p) = self.config_path {
            lines.push(format!("path: {p}"));
        }
        if let Some(ref s) = self.schema {
            lines.push(format!("schema: {s}"));
        }
        if let Some(ref c) = self.component {
            lines.push(format!("component: {c}"));
        }
        if let Some(ref p) = self.platform {
            lines.push(format!("platform: {p}"));
        }
        if let Some(ref c) = self.capability {
            lines.push(format!("capability: {c}"));
        }
        if let Some(ref f) = self.fix_suggestion {
            lines.push(format!("fix: {f}"));
        }
        if let Some(ref id) = self.log_id {
            lines.push(format!("log_id: {id}"));
        }
        lines.join("\n")
    }
}

impl std::fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.summary())
    }
}

impl std::error::Error for DiagnosticError {}

// ── Pre-built error constructors ───────────────────────────────────

impl DiagnosticError {
    /// E-CONFIG-VALIDATION: config validation failed.
    pub fn config_validation(msg: impl Into<String>) -> Self {
        Self::new("E-CONFIG-VALIDATION", Severity::ConfigDeploy, msg)
    }

    /// E-RIME-UNSUPPORTED-COMP: unsupported Rime component.
    pub fn rime_unsupported(comp_type: &str, schema: &str, path: &str) -> Self {
        Self::new(
            "E-RIME-UNSUPPORTED-COMP",
            Severity::ComponentInit,
            format!("Rime component `{comp_type}` is not yet supported in CheIME"),
        )
        .with_component(comp_type)
        .with_schema(schema)
        .with_config_path(path)
        .with_fix("Remove this component from the schema or use a CheIME-supported alternative")
    }

    /// E-COMPONENT-BUILD: pipeline component failed to build.
    pub fn component_build(stage: &str, reason: impl Into<String>) -> Self {
        Self::new(
            "E-COMPONENT-BUILD",
            Severity::ComponentInit,
            format!("Failed to build {stage}: {reason}", reason = reason.into()),
        )
        .with_component(stage)
    }

    /// E-SESSION-STALE: stale message rejected.
    pub fn session_stale(what: &str, reason: impl Into<String>) -> Self {
        Self::new(
            "E-SESSION-STALE",
            Severity::Platform,
            format!("Stale {what}: {reason}", reason = reason.into()),
        )
    }

    /// E-DICT-IMPORT: dictionary import/parse failure.
    pub fn dict_import(file: &str, reason: impl Into<String>) -> Self {
        Self::new(
            "E-DICT-IMPORT",
            Severity::ComponentInit,
            format!("Dictionary import failed: {reason}", reason = reason.into()),
        )
        .with_file(file)
    }

    /// E-DEPLOY-IO: deployment I/O error.
    pub fn deploy_io(op: &str, path: &str, reason: impl Into<String>) -> Self {
        Self::new(
            "E-DEPLOY-IO",
            Severity::ConfigDeploy,
            format!(
                "Deployment {op} failed for {path}: {reason}",
                reason = reason.into()
            ),
        )
        .with_file(path)
        .with_fix("Check filesystem permissions and disk space")
    }

    /// E-PIPELINE-INTEGRITY: invariant violation in pipeline.
    pub fn pipeline_integrity(what: &str) -> Self {
        Self::new(
            "E-PIPELINE-INTEGRITY",
            Severity::Fatal,
            format!("Pipeline invariant violated: {what}"),
        )
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_error_formats_full_diagnostics() {
        let err = DiagnosticError::rime_unsupported(
            "abc_translator",
            "example.schema.yaml",
            "engine/translators/2",
        );
        let detail = err.detailed();
        assert!(detail.contains("E-RIME-UNSUPPORTED-COMP"));
        assert!(detail.contains("abc_translator"));
        assert!(detail.contains("example.schema.yaml"));
        assert!(detail.contains("fix:"));
    }

    #[test]
    fn error_summary_is_compact() {
        let err = DiagnosticError::config_validation("page_size must be >= 1");
        let s = err.to_string();
        assert!(s.starts_with("[E-CONFIG-VALIDATION]"));
        assert!(s.contains("page_size"));
        assert!(s.len() < 80);
    }

    #[test]
    fn json_serialization_omits_none_fields() {
        let err = DiagnosticError::new("E-TEST", Severity::Network, "test message");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"E-TEST\""));
        assert!(!json.contains("\"file\":")); // None fields omitted
        assert!(!json.contains("\"schema\":"));
    }

    #[test]
    fn json_includes_set_fields() {
        let err = DiagnosticError::deploy_io("write", "/tmp/deploy", "permission denied");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"file\":\"/tmp/deploy\""));
        assert!(json.contains("\"fix_suggestion\""));
    }
}
