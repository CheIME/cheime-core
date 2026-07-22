use crate::SessionError;
use cheime_model::{
    ActionId, CandidateId, CandidateSnapshot, PlatformAction, PlatformActionKind,
    PlatformActionOutcome, Revision, SessionStatus, UiCommand,
};
use cheime_pipeline::{InputPipeline, PipelineIntent};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use std::collections::BTreeMap;

/// Default candidates per page (matches Rime convention).
const DEFAULT_PAGE_SIZE: usize = 9;

#[derive(Clone, Debug)]
enum PendingEffect {
    ClearComposition,
    KeepComposition,
}

pub struct Session<P> {
    identity: MessageHeader,
    pipeline: P,
    composition: String,
    /// Full candidate list (before pagination).
    candidates: Vec<cheime_model::Candidate>,
    /// Index into `candidates` of the currently highlighted entry.
    highlighted_idx: usize,
    /// Current page index (0-based).
    page: usize,
    /// Number of candidates shown per page.
    page_size: usize,
    revision: Revision,
    last_sequence: cheime_model::Sequence,
    next_action: u64,
    pending: BTreeMap<ActionId, PendingEffect>,
}

impl<P: InputPipeline> Session<P> {
    #[must_use]
    pub fn new(identity: MessageHeader, pipeline: P) -> Self {
        Self {
            revision: identity.revision,
            last_sequence: identity.sequence,
            identity,
            pipeline,
            composition: String::new(),
            candidates: Vec::new(),
            highlighted_idx: 0,
            page: 0,
            page_size: DEFAULT_PAGE_SIZE,
            next_action: 1,
            pending: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn composition(&self) -> &str {
        &self.composition
    }

    // ── Message dispatch ──────────────────────────────────────────────

    pub fn handle(&mut self, message: FrontendMessage) -> Result<Vec<EngineMessage>, SessionError> {
        self.validate_header(message.header(), &message)?;
        self.last_sequence = message.header().sequence;

        match message {
            FrontendMessage::KeyCommand { event, .. } => self.handle_key(event),
            FrontendMessage::PlatformActionResult { result, .. } => {
                self.handle_action_result(result)
            }
            FrontendMessage::UiCommand { command, .. } => self.handle_ui_command(command),
            FrontendMessage::OpenSession { .. } | FrontendMessage::CloseSession { .. } => {
                Ok(Vec::new())
            }
        }
    }

    // ── Header validation ─────────────────────────────────────────────

    fn validate_header(
        &self,
        header: &MessageHeader,
        message: &FrontendMessage,
    ) -> Result<(), SessionError> {
        if header.epoch != self.identity.epoch {
            return Err(SessionError::StaleEpoch {
                received: header.epoch,
                expected: self.identity.epoch,
            });
        }
        if header.sequence <= self.last_sequence {
            return Err(SessionError::StaleSequence {
                received: header.sequence,
                last: self.last_sequence,
            });
        }
        match &message {
            FrontendMessage::KeyCommand { .. } | FrontendMessage::UiCommand { .. } => {}
            _ => {
                if header.revision != self.revision {
                    return Err(SessionError::StaleRevision {
                        received: header.revision,
                        current: self.revision,
                    });
                }
            }
        }
        Ok(())
    }
    fn handle_key(
        &mut self,
        event: cheime_model::KeyEvent,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        let update = self.pipeline.apply(&self.composition, &event)?;
        match update.intent {
            PipelineIntent::CommitHighlighted => self.propose_commit(),
            PipelineIntent::Cancel => self.propose_cancel(),
            PipelineIntent::CommitText(text) => self.propose_commit_text(&text),
            PipelineIntent::None => {
                self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
                self.composition = update.composition;
                self.candidates = update.candidates;
                // Reset pagination when composition changes
                self.highlighted_idx = 0;
                self.page = 0;

                let action = self.new_action(
                    PlatformActionKind::SetPreedit {
                        text: self.composition.clone(),
                        cursor: self.composition.len(),
                    },
                    PendingEffect::KeepComposition,
                );
                Ok(vec![
                    self.action_message(action),
                    self.snapshot_message(SessionStatus::Composing),
                ])
            }
        }
    }

    // ── UI commands ───────────────────────────────────────────────────

    fn handle_ui_command(
        &mut self,
        command: UiCommand,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        match command {
            UiCommand::SelectCandidate { candidate_id, .. } => {
                // Find the candidate by ID in the full list
                let text = self
                    .candidates
                    .iter()
                    .find(|c| c.id == candidate_id)
                    .map(|c| c.text.clone())
                    .ok_or(SessionError::NoCandidate)?;

                let action = self.new_action(
                    PlatformActionKind::Commit { text },
                    PendingEffect::ClearComposition,
                );
                Ok(vec![
                    self.action_message(action),
                    self.snapshot_message(SessionStatus::CommitPending),
                ])
            }
            UiCommand::MoveHighlight(delta) => {
                if self.candidates.is_empty() {
                    return Ok(Vec::new());
                }
                let max = self.candidates.len().saturating_sub(1);
                let new_idx = if delta < 0 {
                    self.highlighted_idx
                        .saturating_sub(delta.unsigned_abs() as usize)
                } else {
                    (self.highlighted_idx + delta as usize).min(max)
                };
                self.highlighted_idx = new_idx;

                // Auto-flip to the page containing the highlight
                self.page = new_idx / self.page_size;

                Ok(vec![self.snapshot_message(SessionStatus::Composing)])
            }
            UiCommand::NextPage => {
                let total_pages = self.total_pages();
                if total_pages > 0 {
                    self.page = (self.page + 1).min(total_pages - 1);
                    self.highlighted_idx = self.page * self.page_size;
                }
                Ok(vec![self.snapshot_message(SessionStatus::Composing)])
            }
            UiCommand::PreviousPage => {
                self.page = self.page.saturating_sub(1);
                self.highlighted_idx = self.page * self.page_size;
                Ok(vec![self.snapshot_message(SessionStatus::Composing)])
            }
            UiCommand::Dismiss => self.propose_cancel(),
        }
    }

    // ── Pagination helpers ────────────────────────────────────────────

    fn page_candidates(&self) -> Vec<cheime_model::Candidate> {
        let start = self.page * self.page_size;
        if start >= self.candidates.len() {
            return Vec::new();
        }
        let end = (start + self.page_size).min(self.candidates.len());
        self.candidates[start..end].to_vec()
    }

    fn current_highlight_id(&self) -> Option<CandidateId> {
        let start = self.page * self.page_size;
        let page_cands = self.page_candidates();
        let local_idx = self.highlighted_idx.saturating_sub(start);
        page_cands.get(local_idx).map(|c| c.id)
    }

    fn total_pages(&self) -> usize {
        if self.candidates.is_empty() {
            return 0;
        }
        self.candidates.len().div_ceil(self.page_size)
    }

    // ── Commit / Cancel ───────────────────────────────────────────────

    fn propose_commit(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let text = self
            .candidates
            .get(self.highlighted_idx)
            .map(|c| c.text.clone())
            .ok_or(SessionError::NoCandidate)?;
        let action = self.new_action(
            PlatformActionKind::Commit { text },
            PendingEffect::ClearComposition,
        );
        Ok(vec![
            self.action_message(action),
            self.snapshot_message(SessionStatus::CommitPending),
        ])
    }

    fn propose_cancel(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let action = self.new_action(
            PlatformActionKind::CancelComposition,
            PendingEffect::ClearComposition,
        );
        Ok(vec![self.action_message(action)])
    }
    fn propose_commit_text(&mut self, text: &str) -> Result<Vec<EngineMessage>, SessionError> {
        let action = self.new_action(
            PlatformActionKind::Commit {
                text: text.to_owned(),
            },
            PendingEffect::ClearComposition,
        );
        Ok(vec![
            self.action_message(action),
            self.snapshot_message(SessionStatus::CommitPending),
        ])
    }
    fn handle_action_result(
        &mut self,
        result: cheime_model::PlatformActionResult,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        let effect = self
            .pending
            .remove(&result.action_id)
            .ok_or(SessionError::UnknownAction(result.action_id))?;
        if matches!(result.outcome, PlatformActionOutcome::Applied)
            && matches!(effect, PendingEffect::ClearComposition)
        {
            self.composition.clear();
            self.candidates.clear();
            self.highlighted_idx = 0;
            self.page = 0;
            self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
            return Ok(vec![self.snapshot_message(SessionStatus::Ready)]);
        }
        Ok(Vec::new())
    }

    // ── Helpers ───────────────────────────────────────────────────────

    fn new_action(&mut self, kind: PlatformActionKind, effect: PendingEffect) -> PlatformAction {
        let id = ActionId::new(self.next_action);
        self.next_action += 1;
        self.pending.insert(id, effect);
        PlatformAction {
            id,
            epoch: self.identity.epoch,
            revision: self.revision,
            kind,
        }
    }

    fn action_message(&self, action: PlatformAction) -> EngineMessage {
        EngineMessage::PlatformAction {
            header: self.output_header(),
            action,
        }
    }

    fn snapshot_message(&self, status: SessionStatus) -> EngineMessage {
        EngineMessage::CandidateSnapshot {
            header: self.output_header(),
            snapshot: CandidateSnapshot {
                epoch: self.identity.epoch,
                revision: self.revision,
                deployment: self.identity.deployment,
                preedit: self.composition.clone(),
                cursor: self.composition.len(),
                candidates: self.page_candidates(),
                highlighted: self.current_highlight_id(),
                status,
                page_size: self.page_size,
                page: self.page,
            },
        }
    }

    fn output_header(&self) -> MessageHeader {
        MessageHeader {
            protocol_version: self.identity.protocol_version,
            client: self.identity.client,
            session: self.identity.session,
            epoch: self.identity.epoch,
            sequence: self.last_sequence,
            revision: self.revision,
            deployment: self.identity.deployment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{
        CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
        PlatformActionOutcome, PlatformActionResult, Revision, Sequence, SessionEpoch, SessionId,
    };
    use cheime_pipeline::BuiltinPipeline;

    fn initial_header() -> MessageHeader {
        MessageHeader {
            protocol_version: CORE_PROTOCOL_VERSION,
            client: ClientInstanceId::new(1),
            session: SessionId::new(2),
            epoch: SessionEpoch::new(3),
            sequence: Sequence::new(0),
            revision: Revision::new(0),
            deployment: DeploymentGeneration::new(4),
        }
    }

    fn pipeline() -> BuiltinPipeline {
        BuiltinPipeline::new([
            (String::from("ni"), String::from("你"), 10),
            (String::from("ni"), String::from("尼"), 5),
            (String::from("ni"), String::from("泥"), 3),
        ])
    }

    fn key_message(sequence: u64, revision: u64, key: Key) -> FrontendMessage {
        let mut header = initial_header();
        header.sequence = Sequence::new(sequence);
        header.revision = Revision::new(revision);
        FrontendMessage::KeyCommand {
            header,
            event: KeyEvent {
                key,
                state: KeyState::default(),
            },
        }
    }

    fn ui_message(seq: u64, rev: u64, command: UiCommand) -> FrontendMessage {
        let mut header = initial_header();
        header.sequence = Sequence::new(seq);
        header.revision = Revision::new(rev);
        FrontendMessage::UiCommand { header, command }
    }

    #[test]
    fn key_command_publishes_preedit_action_and_snapshot() {
        let mut session = Session::new(initial_header(), pipeline());
        let output = session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        assert_eq!(output.len(), 2);
        assert!(matches!(
            &output[0],
            EngineMessage::PlatformAction { action, .. }
                if matches!(&action.kind, PlatformActionKind::SetPreedit { text, cursor } if text == "n" && *cursor == 1)
        ));
        assert!(matches!(
            &output[1],
            EngineMessage::CandidateSnapshot { snapshot, .. }
                if snapshot.preedit == "n" && snapshot.revision == Revision::new(1)
        ));
    }

    #[test]
    fn commit_remains_pending_until_platform_applies_it() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        let output = session.handle(key_message(3, 2, Key::Enter)).unwrap();
        let action_id = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => {
                assert_eq!(
                    action.kind,
                    PlatformActionKind::Commit {
                        text: String::from("你")
                    }
                );
                action.id
            }
            other => panic!("expected platform action, got {other:?}"),
        };
        assert_eq!(session.composition(), "ni");

        let mut result_header = initial_header();
        result_header.sequence = Sequence::new(4);
        result_header.revision = Revision::new(2);
        session
            .handle(FrontendMessage::PlatformActionResult {
                header: result_header,
                result: PlatformActionResult {
                    action_id,
                    outcome: PlatformActionOutcome::Applied,
                },
            })
            .unwrap();
        assert_eq!(session.composition(), "");
    }

    #[test]
    fn stale_epoch_is_rejected_without_mutating_composition() {
        let mut session = Session::new(initial_header(), pipeline());
        let mut message = key_message(1, 0, Key::Character('n'));
        match &mut message {
            FrontendMessage::KeyCommand { header, .. } => {
                header.epoch = SessionEpoch::new(99);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            session.handle(message),
            Err(SessionError::StaleEpoch {
                received: SessionEpoch::new(99),
                expected: SessionEpoch::new(3),
            })
        );
        assert_eq!(session.composition(), "");
    }

    #[test]
    fn ui_select_candidate_commits_specific_text() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        // Now candidates: [你(id=1), 尼(id=2), 泥(id=3)]
        let output = session
            .handle(ui_message(
                3,
                2,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(2),
                    candidate_id: CandidateId::new(2),
                },
            ))
            .unwrap();
        let action = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => action.clone(),
            other => panic!("expected PlatformAction, got {other:?}"),
        };
        assert_eq!(
            action.kind,
            PlatformActionKind::Commit {
                text: String::from("尼")
            }
        );
    }

    #[test]
    fn ui_move_highlight_shifts_selection() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        // Highlight defaults to 0 (你). Move down by 1 → 尼.
        let output = session
            .handle(ui_message(3, 2, UiCommand::MoveHighlight(1)))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.highlighted, Some(CandidateId::new(2)));
    }

    #[test]
    fn page_up_down_navigates_pages() {
        let mut session = Session::new(initial_header(), pipeline());
        // Query gives 3 candidates, page_size=9 → 1 page
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();

        // Next page should clamp (only 1 page)
        let output = session
            .handle(ui_message(3, 2, UiCommand::NextPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 0);
    }

    #[test]
    fn propose_commit_commits_highlighted_not_first() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        // Move highlight to candidate #2 (尼)
        session
            .handle(ui_message(3, 2, UiCommand::MoveHighlight(1)))
            .unwrap();
        // Now Enter should commit the highlighted candidate
        let output = session.handle(key_message(4, 2, Key::Enter)).unwrap();
        let action = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => action.clone(),
            other => panic!("expected PlatformAction, got {other:?}"),
        };
        assert_eq!(
            action.kind,
            PlatformActionKind::Commit {
                text: String::from("尼")
            }
        );
    }

    #[test]
    fn page_navigation_at_boundaries() {
        let mut session = Session::new(initial_header(), pipeline());
        // No candidates yet — previous/next page should no-op gracefully
        let output = session
            .handle(ui_message(1, 0, UiCommand::PreviousPage))
            .unwrap();
        // Should still get a snapshot
        assert_eq!(output.len(), 1);
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 0);

        // NextPage on empty candidates likewise should be safe
        let output = session
            .handle(ui_message(2, 0, UiCommand::NextPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 0);

        // Now type something to get candidates
        session
            .handle(key_message(3, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(4, 1, Key::Character('i')))
            .unwrap();
        // 3 candidates, page_size=9 → 1 page. Page down at last page should clamp.
        let output = session
            .handle(ui_message(5, 2, UiCommand::NextPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 0); // only 1 page
    }

    #[test]
    fn commit_with_empty_candidates() {
        // Create a session with a minimal pipeline that has no dictionary
        let mut session = Session::new(initial_header(), BuiltinPipeline::new([]));
        // With no candidates, proposing a commit should fail
        let result = session.handle(key_message(1, 0, Key::Enter));
        assert!(matches!(result, Err(SessionError::NoCandidate)));
    }

    #[test]
    fn highlight_wraps_on_page_navigation() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        // 3 candidates, page_size=9 → all on page 0
        // Move highlight to candidate #2 (index 2)
        session
            .handle(ui_message(3, 2, UiCommand::MoveHighlight(2)))
            .unwrap();
        // Now highlighted_idx is 2, page is 0
        let output = session
            .handle(ui_message(4, 2, UiCommand::NextPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        // Page unchanged (only 1 page), but highlighted resets to page start
        assert_eq!(snapshot.page, 0);
        assert_eq!(snapshot.highlighted, Some(CandidateId::new(1)));
    }

    #[test]
    fn dismiss_clears_composition_and_candidates() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        assert_eq!(session.composition(), "ni");
        assert!(!session.candidates.is_empty());

        // Dismiss sends CancelComposition, waits for platform to apply
        let output = session
            .handle(ui_message(3, 2, UiCommand::Dismiss))
            .unwrap();
        let action_id = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => {
                assert_eq!(action.kind, PlatformActionKind::CancelComposition);
                action.id
            }
            other => panic!("expected PlatformAction, got {other:?}"),
        };
        // Composition preserved until platform applies
        assert_eq!(session.composition(), "ni");

        // Platform applies the dismiss
        let mut result_header = initial_header();
        result_header.sequence = Sequence::new(4);
        result_header.revision = Revision::new(2);
        session
            .handle(FrontendMessage::PlatformActionResult {
                header: result_header,
                result: PlatformActionResult {
                    action_id,
                    outcome: PlatformActionOutcome::Applied,
                },
            })
            .unwrap();
        assert_eq!(session.composition(), "");
        assert!(session.candidates.is_empty());
    }

    #[test]
    fn rejected_commit_preserves_composition() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();

        // Select candidate #1 (你) — this creates a Commit action
        let output = session
            .handle(ui_message(
                3,
                2,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(2),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        let action_id = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => action.id,
            other => panic!("expected PlatformAction, got {other:?}"),
        };
        assert_eq!(session.composition(), "ni");

        // Platform rejects the commit
        let mut result_header = initial_header();
        result_header.sequence = Sequence::new(4);
        result_header.revision = Revision::new(2);
        let output = session
            .handle(FrontendMessage::PlatformActionResult {
                header: result_header,
                result: PlatformActionResult {
                    action_id,
                    outcome: PlatformActionOutcome::Rejected {
                        reason: String::from("stale epoch"),
                    },
                },
            })
            .unwrap();
        assert!(output.is_empty());
        // Composition and candidates must be preserved after rejection
        assert_eq!(session.composition(), "ni");
        assert!(!session.candidates.is_empty());
    }

    #[test]
    fn multi_page_navigation_with_many_candidates() {
        let big = {
            let mut entries = Vec::new();
            for i in 0..15 {
                entries.push((String::from("ni"), format!("你{}", i), 15 - i));
            }
            BuiltinPipeline::new(entries)
        };
        let mut session = Session::new(initial_header(), big);
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();

        // 15 candidates, page_size=9 → page 0 has 9
        let output = session
            .handle(ui_message(3, 2, UiCommand::NextPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 1);
        // Page 1 has remaining 6 candidates
        assert_eq!(snapshot.candidates.len(), 6);

        // PreviousPage goes back to page 0
        let output = session
            .handle(ui_message(4, 2, UiCommand::PreviousPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 0);
        assert_eq!(snapshot.candidates.len(), 9);

        // PreviousPage at page 0 clamps (no negative page)
        let output = session
            .handle(ui_message(5, 2, UiCommand::PreviousPage))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.page, 0);
    }

    #[test]
    fn backspace_and_retype_produces_same_candidates() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        let output = session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();

        // Record candidates from first "ni" query
        let first_snapshot = match &output[1] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot.clone(),
            other => panic!("expected snapshot, got {other:?}"),
        };

        // Backspace to "n"
        session.handle(key_message(3, 2, Key::Backspace)).unwrap();
        assert_eq!(session.composition(), "n");

        // Retype "i" → should produce same candidates
        let output = session
            .handle(key_message(4, 3, Key::Character('i')))
            .unwrap();
        let second_snapshot = match &output[1] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot.clone(),
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(first_snapshot.candidates, second_snapshot.candidates);
    }

    #[test]
    fn move_highlight_past_candidate_count_clamps() {
        let mut session = Session::new(initial_header(), pipeline());
        session
            .handle(key_message(1, 0, Key::Character('n')))
            .unwrap();
        session
            .handle(key_message(2, 1, Key::Character('i')))
            .unwrap();
        // 3 candidates: indices 0=你, 1=尼, 2=泥

        // MoveHighlight(5) with only 3 candidates → clamps to last (index 2)
        let output = session
            .handle(ui_message(3, 2, UiCommand::MoveHighlight(5)))
            .unwrap();
        let snapshot = match &output[0] {
            EngineMessage::CandidateSnapshot { snapshot, .. } => snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        // Clamped to last candidate: index 2 → CandidateId 3 (泥)
        assert_eq!(snapshot.highlighted, Some(CandidateId::new(3)));
    }
}
