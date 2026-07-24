use super::editor::Document;
use cheime_model::{CandidateSnapshot, PlatformAction, PlatformActionKind, SessionStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LocalAction {
    Insert(char),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    SetStatus(&'static str),
    ClearStatus,
}

pub(super) struct AppState {
    document: Document,
    snapshot: Option<CandidateSnapshot>,
    status: Option<String>,
}

impl AppState {
    pub(super) fn new() -> Self {
        Self {
            document: Document::new(),
            snapshot: None,
            status: None,
        }
    }

    pub(super) fn document(&self) -> &Document {
        &self.document
    }

    pub(super) fn snapshot(&self) -> Option<&CandidateSnapshot> {
        self.snapshot.as_ref()
    }

    pub(super) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(super) fn has_composition(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            !snapshot.preedit.is_empty() || snapshot.status == SessionStatus::CommitPending
        })
    }

    pub(super) fn set_snapshot(&mut self, snapshot: CandidateSnapshot) {
        self.snapshot = Some(snapshot);
    }

    pub(super) fn set_status(&mut self, status: impl Into<String>) {
        self.status = Some(status.into());
    }

    pub(super) fn apply_platform_action(&mut self, action: &PlatformAction) -> Option<String> {
        match &action.kind {
            PlatformActionKind::Commit { text } => {
                self.document.insert(text);
                Some(text.clone())
            }
            PlatformActionKind::SetPreedit { .. } | PlatformActionKind::CancelComposition => None,
        }
    }

    pub(super) fn apply_local(&mut self, action: LocalAction) {
        match action {
            LocalAction::Insert(character) if !self.has_composition() => {
                let mut buffer = [0_u8; 4];
                self.document.insert(character.encode_utf8(&mut buffer));
            }
            LocalAction::Backspace if !self.has_composition() => {
                self.document.backspace();
            }
            LocalAction::Delete if !self.has_composition() => {
                self.document.delete();
            }
            LocalAction::MoveLeft if !self.has_composition() => self.document.move_left(),
            LocalAction::MoveRight if !self.has_composition() => self.document.move_right(),
            LocalAction::MoveHome if !self.has_composition() => self.document.move_home(),
            LocalAction::MoveEnd if !self.has_composition() => self.document.move_end(),
            LocalAction::SetStatus(status) => self.status = Some(status.into()),
            LocalAction::ClearStatus => self.status = None,
            LocalAction::Insert(_)
            | LocalAction::Backspace
            | LocalAction::Delete
            | LocalAction::MoveLeft
            | LocalAction::MoveRight
            | LocalAction::MoveHome
            | LocalAction::MoveEnd => {}
        }
    }
}

#[cfg(test)]
mod app_tests;
