use crate::{ActionId, CandidateId, Revision, SessionEpoch, SessionId};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct CommitToken {
    pub session: SessionId,
    pub epoch: SessionEpoch,
    pub action_id: ActionId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Key {
    Character(char),
    Backspace,
    Escape,
    Enter,
    Space,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyState {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyEvent {
    pub key: Key,
    pub state: KeyState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UiCommand {
    SelectCandidate {
        epoch: SessionEpoch,
        snapshot_revision: Revision,
        candidate_id: CandidateId,
    },
    MoveHighlight(i32),
    NextPage,
    PreviousPage,
    Dismiss,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlatformActionKind {
    SetPreedit { text: String, cursor: usize },
    Commit { text: String },
    CancelComposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlatformAction {
    pub id: ActionId,
    pub epoch: SessionEpoch,
    pub revision: Revision,
    pub kind: PlatformActionKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlatformActionOutcome {
    Applied,
    Rejected { reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlatformActionResult {
    pub action_id: ActionId,
    pub outcome: PlatformActionOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_selection_carries_snapshot_identity() {
        let command = UiCommand::SelectCandidate {
            epoch: SessionEpoch::new(4),
            snapshot_revision: Revision::new(9),
            candidate_id: CandidateId::new(12),
        };
        assert_eq!(
            command,
            UiCommand::SelectCandidate {
                epoch: SessionEpoch::new(4),
                snapshot_revision: Revision::new(9),
                candidate_id: CandidateId::new(12),
            }
        );
    }
}
