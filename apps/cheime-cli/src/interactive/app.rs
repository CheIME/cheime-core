//! CLI-local application state core.
//!
//! Owns the document buffer, the latest immutable candidate snapshot, detail
//! display state, an optional transient status line, and the exit flag.
//! Predicates (`has_composition`, `commit_pending`) are derived from the
//! latest snapshot so that rendering and input routing can decide without
//! inspecting the snapshot internals directly.

use super::editor::Document;
use cheime_model::{CandidateSnapshot, PlatformAction, PlatformActionKind, SessionStatus};

// ── DetailMode ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DetailMode {
    Parsed,
    Json,
}

// ── LocalAction ─────────────────────────────────────────────────────────

/// Local reducer actions scoped to the interactive CLI state.
///
/// Edit and movement variants only mutate the document when composition is
/// **not** active (i.e. `has_composition()` returns `false`).  View and status
/// actions always apply, regardless of composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LocalAction {
    Insert(char),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    ToggleDetailMode,
    ScrollUp,
    ScrollDown,
    SetStatus(&'static str),
    ClearStatus,
}

/// The local document effect of applying one engine platform action.
///
/// The original action remains available for the acknowledgement slice.  A
/// commit also carries its text for the later exactly-once learning slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PlatformActionApplication {
    NoDocumentChange {
        action: PlatformAction,
    },
    Committed {
        action: PlatformAction,
        text: String,
    },
}

// ── AppState ──────────────────────────────────────────────────────────────

pub(super) struct AppState {
    document: Document,
    snapshot: Option<CandidateSnapshot>,
    detail_mode: DetailMode,
    detail_scroll: usize,
    status: Option<String>,
    #[allow(dead_code)]
    should_exit: bool,
}

impl AppState {
    // ── construction ──────────────────────────────────────────────────

    pub(super) fn new() -> Self {
        Self {
            document: Document::new(),
            snapshot: None,
            detail_mode: DetailMode::Parsed,
            detail_scroll: 0,
            status: None,
            should_exit: false,
        }
    }

    // ── accessors ─────────────────────────────────────────────────────

    pub(super) fn document(&self) -> &Document {
        &self.document
    }

    pub(super) fn snapshot(&self) -> Option<&CandidateSnapshot> {
        self.snapshot.as_ref()
    }

    pub(super) fn detail_mode(&self) -> DetailMode {
        self.detail_mode
    }

    pub(super) fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    pub(super) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    #[allow(dead_code)]
    pub(super) fn should_exit(&self) -> bool {
        self.should_exit
    }

    // ── predicates ────────────────────────────────────────────────────

    /// Composition is active when the latest snapshot has non-empty preedit
    /// text OR its status is `CommitPending` (the commit has been proposed
    /// but not yet confirmed by the platform).
    pub(super) fn has_composition(&self) -> bool {
        match &self.snapshot {
            None => false,
            Some(snap) => !snap.preedit.is_empty() || snap.status == SessionStatus::CommitPending,
        }
    }

    /// `true` when the latest snapshot status is `CommitPending`.
    /// Used to block local edits while the platform has not acknowledged
    /// the commit.
    #[allow(dead_code)]
    pub(super) fn commit_pending(&self) -> bool {
        match &self.snapshot {
            None => false,
            Some(snap) => snap.status == SessionStatus::CommitPending,
        }
    }

    // ── mutators ──────────────────────────────────────────────────────

    /// Replace the latest immutable snapshot.
    pub(super) fn set_snapshot(&mut self, snap: CandidateSnapshot) {
        self.snapshot = Some(snap);
    }

    /// Replace the transient status line.
    pub(super) fn set_status(&mut self, status: impl Into<String>) {
        self.status = Some(status.into());
    }

    /// Flag the application to exit at the next opportunity.
    #[allow(dead_code)]
    pub(super) fn set_should_exit(&mut self) {
        self.should_exit = true;
    }

    /// Apply the CLI-local document effect of one platform action.
    ///
    /// This deliberately bypasses [`Self::apply_local`]'s composition guard:
    /// commits are applied while the core session is `CommitPending`.
    pub(super) fn apply_platform_action(
        &mut self,
        action: PlatformAction,
    ) -> PlatformActionApplication {
        match &action.kind {
            PlatformActionKind::SetPreedit { .. } | PlatformActionKind::CancelComposition => {
                PlatformActionApplication::NoDocumentChange { action }
            }
            PlatformActionKind::Commit { text } => {
                let committed_text = text.clone();
                self.document.insert(&committed_text);
                PlatformActionApplication::Committed {
                    action,
                    text: committed_text,
                }
            }
        }
    }

    // ── local reducer ─────────────────────────────────────────────────

    /// Apply a pure local action.
    ///
    /// Edit and movement actions are skipped when composition is active
    /// (non-empty preedit or `CommitPending`).  View and status actions
    /// always execute.
    pub(super) fn apply_local(&mut self, action: LocalAction) {
        match action {
            // ── edit / movement (blocked during composition) ──────────
            LocalAction::Insert(ch) => {
                if !self.has_composition() {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    self.document.insert(s);
                }
            }
            LocalAction::Backspace => {
                if !self.has_composition() {
                    self.document.backspace();
                }
            }
            LocalAction::Delete => {
                if !self.has_composition() {
                    self.document.delete();
                }
            }
            LocalAction::MoveLeft => {
                if !self.has_composition() {
                    self.document.move_left();
                }
            }
            LocalAction::MoveRight => {
                if !self.has_composition() {
                    self.document.move_right();
                }
            }
            LocalAction::MoveHome => {
                if !self.has_composition() {
                    self.document.move_home();
                }
            }
            LocalAction::MoveEnd => {
                if !self.has_composition() {
                    self.document.move_end();
                }
            }

            // ── view / status (always apply) ──────────────────────────
            LocalAction::ToggleDetailMode => {
                self.detail_mode = match self.detail_mode {
                    DetailMode::Parsed => DetailMode::Json,
                    DetailMode::Json => DetailMode::Parsed,
                };
            }
            LocalAction::ScrollUp => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
            }
            LocalAction::ScrollDown => {
                self.detail_scroll = self.detail_scroll.saturating_add(1);
            }
            LocalAction::SetStatus(status) => {
                self.status = Some(status.into());
            }
            LocalAction::ClearStatus => {
                self.status = None;
            }
        }
    }
}

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod blocking_tests;
#[cfg(test)]
mod platform_action_tests;
#[cfg(test)]
mod reducer_tests;
