use crate::SessionError;
use cheime_model::{
    ActionId, CandidateId, CandidateSnapshot, CommitToken, PlatformAction, PlatformActionKind,
    PlatformActionOutcome, Revision, SessionStatus, UiCommand,
};
use cheime_pipeline::decoder::{ResolvedCandidate, SelectedLexeme};
use cheime_pipeline::segmentation::InputSpan;
use cheime_pipeline::{CommitRecord, InputPipeline, PipelineIntent};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use std::collections::BTreeMap;

/// Default candidates per page (matches Rime convention).
const DEFAULT_PAGE_SIZE: usize = 9;

#[derive(Clone, Debug)]
enum PendingEffect {
    ClearComposition { commit: Option<CommitRecord> },
    KeepComposition,
}

#[derive(Clone, Debug)]
struct ConfirmedSegment {
    text: String,
    raw_span: InputSpan,
    canonical_code: String,
    lexemes: Vec<SelectedLexeme>,
}

#[derive(Clone, Debug, Default)]
struct CompositionState {
    raw: String,
    active_start: usize,
    confirmed: Vec<ConfirmedSegment>,
}

impl CompositionState {
    fn active_input(&self) -> &str {
        &self.raw[self.active_start..]
    }

    fn display_text(&self) -> String {
        let mut display = self
            .confirmed
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<String>();
        display.push_str(self.active_input());
        display
    }

    fn confirmed_text(&self) -> String {
        self.confirmed
            .iter()
            .map(|segment| segment.text.as_str())
            .collect()
    }

    fn replace_active(&mut self, active: &str) {
        self.raw.truncate(self.active_start);
        self.raw.push_str(active);
    }

    fn clear(&mut self) {
        self.raw.clear();
        self.active_start = 0;
        self.confirmed.clear();
    }
}

pub struct Session<P> {
    identity: MessageHeader,
    pipeline: P,
    composition: CompositionState,
    /// Full candidate list (before pagination).
    candidates: Vec<ResolvedCandidate>,
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
            composition: CompositionState::default(),
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
        &self.composition.raw
    }

    #[must_use]
    pub fn composition_text(&self) -> String {
        self.composition.display_text()
    }

    #[must_use]
    pub fn active_input(&self) -> &str {
        self.composition.active_input()
    }

    #[cfg(test)]
    fn confirmed_segments(&self) -> &[ConfirmedSegment] {
        &self.composition.confirmed
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
            FrontendMessage::RollbackLearning { token, .. } => {
                if token.session != self.identity.session {
                    return Err(SessionError::WrongSession {
                        received: token.session,
                        expected: self.identity.session,
                    });
                }
                if token.epoch != self.identity.epoch {
                    return Err(SessionError::StaleEpoch {
                        received: token.epoch,
                        expected: self.identity.epoch,
                    });
                }
                self.pipeline.rollback_learning(token);
                Ok(Vec::new())
            }
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
        if header.session != self.identity.session {
            return Err(SessionError::WrongSession {
                received: header.session,
                expected: self.identity.session,
            });
        }
        // PlatformActionResult is a response to our action — its sequence is
        // assigned by the TIP and may not follow the engine's sequence counter.
        // Skip sequence/revision validation for it.
        if matches!(message, FrontendMessage::PlatformActionResult { .. }) {
            return Ok(());
        }
        if header.sequence <= self.last_sequence {
            return Err(SessionError::StaleSequence {
                received: header.sequence,
                last: self.last_sequence,
            });
        }
        match &message {
            FrontendMessage::KeyCommand { .. }
            | FrontendMessage::UiCommand { .. }
            | FrontendMessage::RollbackLearning { .. } => {}
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
        if matches!(event.key, cheime_model::Key::Backspace)
            && self.composition.active_input().is_empty()
            && !self.composition.confirmed.is_empty()
        {
            return self.reopen_last_confirmed();
        }

        let update = self
            .pipeline
            .apply(self.composition.active_input(), &event)?;
        match update.intent {
            PipelineIntent::CommitHighlighted => self.propose_commit(),
            PipelineIntent::CommitRaw => self.propose_commit_raw(),
            PipelineIntent::Cancel => self.propose_cancel(),
            PipelineIntent::CommitText(text) => self.propose_commit_text(&text),
            PipelineIntent::None => {
                self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
                self.composition.replace_active(&update.composition);
                self.candidates = update.candidates;
                // Reset pagination when composition changes
                self.highlighted_idx = 0;
                self.page = 0;

                let preedit = self.composition.display_text();
                let action = self.new_action(
                    PlatformActionKind::SetPreedit {
                        cursor: preedit.len(),
                        text: preedit,
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
                let candidate = self
                    .candidates
                    .iter()
                    .find(|c| c.id == candidate_id)
                    .cloned()
                    .ok_or(SessionError::NoCandidate)?;
                self.select_candidate(candidate)
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
        self.candidates[start..end]
            .iter()
            .map(|candidate| candidate.display.clone())
            .collect()
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
        if let Some(candidate) = self.candidates.get(self.highlighted_idx).cloned() {
            return self.select_candidate(candidate);
        }
        let text = self.composition.display_text();
        if text.is_empty() {
            // Nothing to commit — just cancel the composition silently.
            return self.propose_cancel();
        }
        let action = self.new_action(
            PlatformActionKind::Commit { text },
            PendingEffect::ClearComposition { commit: None },
        );
        Ok(vec![
            self.action_message(action),
            self.snapshot_message(SessionStatus::CommitPending),
        ])
    }

    fn propose_cancel(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let action = self.new_action(
            PlatformActionKind::CancelComposition,
            PendingEffect::ClearComposition { commit: None },
        );
        Ok(vec![self.action_message(action)])
    }
    fn propose_commit_text(&mut self, text: &str) -> Result<Vec<EngineMessage>, SessionError> {
        let text = format!("{}{}", self.composition.confirmed_text(), text);
        let action = self.new_action(
            PlatformActionKind::Commit { text },
            PendingEffect::ClearComposition { commit: None },
        );
        Ok(vec![
            self.action_message(action),
            self.snapshot_message(SessionStatus::CommitPending),
        ])
    }
    /// Commit the raw composition text as-is (Enter key / predict mode).
    fn propose_commit_raw(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let text = self.composition.display_text();
        if text.is_empty() {
            return self.propose_cancel();
        }
        let action = self.new_action(
            PlatformActionKind::Commit { text },
            PendingEffect::ClearComposition { commit: None },
        );
        Ok(vec![
            self.action_message(action),
            self.snapshot_message(SessionStatus::CommitPending),
        ])
    }

    fn select_candidate(
        &mut self,
        candidate: ResolvedCandidate,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        if candidate.complete {
            let text = format!("{}{}", self.composition.confirmed_text(), candidate.text);
            let record = self.commit_record(&candidate);
            let action = self.new_action(
                PlatformActionKind::Commit { text },
                PendingEffect::ClearComposition {
                    commit: Some(record),
                },
            );
            return Ok(vec![
                self.action_message(action),
                self.snapshot_message(SessionStatus::CommitPending),
            ]);
        }

        let active_len = self.composition.active_input().len();
        if candidate.consumed.start != 0
            || candidate.consumed.end == 0
            || candidate.consumed.end > active_len
        {
            return Err(SessionError::NoCandidate);
        }
        let absolute_start = self.composition.active_start;
        let absolute_end = absolute_start + candidate.consumed.end;
        self.composition.confirmed.push(ConfirmedSegment {
            text: candidate.text.clone(),
            raw_span: InputSpan::new(absolute_start, absolute_end),
            canonical_code: candidate.canonical_code,
            lexemes: candidate.lexemes,
        });
        self.composition.active_start = absolute_end;
        self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
        self.refresh_active_candidates()?;
        self.preedit_update()
    }

    fn reopen_last_confirmed(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let Some(segment) = self.composition.confirmed.pop() else {
            return Ok(Vec::new());
        };
        self.composition.active_start = segment.raw_span.start;
        self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
        self.refresh_active_candidates()?;
        self.preedit_update()
    }

    fn refresh_active_candidates(&mut self) -> Result<(), SessionError> {
        self.candidates = self.pipeline.refresh(self.composition.active_input())?;
        self.highlighted_idx = 0;
        self.page = 0;
        Ok(())
    }

    fn preedit_update(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let preedit = self.composition.display_text();
        let action = self.new_action(
            PlatformActionKind::SetPreedit {
                cursor: preedit.len(),
                text: preedit,
            },
            PendingEffect::KeepComposition,
        );
        Ok(vec![
            self.action_message(action),
            self.snapshot_message(SessionStatus::Composing),
        ])
    }

    fn commit_record(&self, candidate: &ResolvedCandidate) -> CommitRecord {
        let mut canonical_codes: Vec<&str> = self
            .composition
            .confirmed
            .iter()
            .map(|segment| segment.canonical_code.as_str())
            .collect();
        canonical_codes.push(candidate.canonical_code.as_str());
        let mut lexemes = self
            .composition
            .confirmed
            .iter()
            .flat_map(|segment| segment.lexemes.clone())
            .collect::<Vec<_>>();
        lexemes.extend(candidate.lexemes.clone());
        CommitRecord {
            text: format!("{}{}", self.composition.confirmed_text(), candidate.text),
            canonical_code: canonical_codes.join(" "),
            schema: self.pipeline.schema_id().to_owned(),
            lexemes,
            exact_phrase: self.composition.confirmed.is_empty() && candidate.exact_phrase,
        }
    }

    fn handle_action_result(
        &mut self,
        result: cheime_model::PlatformActionResult,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        let effect = self
            .pending
            .remove(&result.action_id)
            .ok_or(SessionError::UnknownAction(result.action_id))?;
        if matches!(result.outcome, PlatformActionOutcome::Applied) {
            let PendingEffect::ClearComposition { commit } = effect else {
                return Ok(Vec::new());
            };
            if let Some(record) = commit {
                self.pipeline.commit_applied(
                    CommitToken {
                        session: self.identity.session,
                        epoch: self.identity.epoch,
                        action_id: result.action_id,
                    },
                    record,
                );
            }
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
        let preedit = self.composition.display_text();
        EngineMessage::CandidateSnapshot {
            header: self.output_header(),
            snapshot: CandidateSnapshot {
                epoch: self.identity.epoch,
                revision: self.revision,
                deployment: self.identity.deployment,
                cursor: preedit.len(),
                preedit,
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
    use cheime_pipeline::decoder::{ResolvedCandidate, SelectedLexeme};
    use cheime_pipeline::segmentation::InputSpan;
    use cheime_pipeline::{
        CommitRecord, InputPipeline, PipelineError, PipelineIntent, PipelineUpdate,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Default)]
    struct PartialPipeline;

    impl PartialPipeline {
        fn candidates(composition: &str) -> Vec<ResolvedCandidate> {
            match composition {
                "nihao" => vec![ResolvedCandidate {
                    display: cheime_model::Candidate::text(CandidateId::new(1), "旎", "test"),
                    consumed: InputSpan::new(0, 2),
                    canonical_code: String::from("ni"),
                    lexemes: vec![SelectedLexeme::test("旎", "ni")],
                    complete: false,
                    exact_phrase: true,
                    completion: false,
                    score: 90,
                }],
                "hao" => vec![ResolvedCandidate {
                    display: cheime_model::Candidate::text(CandidateId::new(1), "皓", "test"),
                    consumed: InputSpan::new(0, 3),
                    canonical_code: String::from("hao"),
                    lexemes: vec![SelectedLexeme::test("皓", "hao")],
                    complete: true,
                    exact_phrase: true,
                    completion: false,
                    score: 80,
                }],
                _ => Vec::new(),
            }
        }
    }

    impl InputPipeline for PartialPipeline {
        fn apply(
            &self,
            composition: &str,
            event: &KeyEvent,
        ) -> Result<PipelineUpdate, PipelineError> {
            let mut next = composition.to_owned();
            match event.key {
                Key::Character(character) => next.push(character),
                Key::Backspace => {
                    next.pop();
                }
                _ => {}
            }
            Ok(PipelineUpdate {
                candidates: Self::candidates(&next),
                composition: next,
                intent: PipelineIntent::None,
            })
        }

        fn refresh(&self, composition: &str) -> Result<Vec<ResolvedCandidate>, PipelineError> {
            Ok(Self::candidates(composition))
        }
    }

    #[derive(Clone, Default)]
    struct RecordingLearningPipeline {
        staged: Arc<Mutex<Vec<(cheime_model::CommitToken, CommitRecord)>>>,
        cancel_attempts: Arc<Mutex<usize>>,
        cancelled: Arc<Mutex<Vec<cheime_model::CommitToken>>>,
    }

    impl InputPipeline for RecordingLearningPipeline {
        fn apply(
            &self,
            composition: &str,
            event: &KeyEvent,
        ) -> Result<PipelineUpdate, PipelineError> {
            PartialPipeline.apply(composition, event)
        }

        fn refresh(&self, composition: &str) -> Result<Vec<ResolvedCandidate>, PipelineError> {
            PartialPipeline.refresh(composition)
        }

        fn commit_applied(&self, token: cheime_model::CommitToken, record: CommitRecord) {
            self.staged.lock().unwrap().push((token, record));
        }

        fn rollback_learning(&self, token: cheime_model::CommitToken) {
            *self.cancel_attempts.lock().unwrap() += 1;
            let mut cancelled = self.cancelled.lock().unwrap();
            if !cancelled.contains(&token) {
                cancelled.push(token);
            }
        }
    }

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

    fn type_text<P: InputPipeline>(
        session: &mut Session<P>,
        text: &str,
        mut sequence: u64,
        mut revision: u64,
    ) -> (u64, u64) {
        for character in text.chars() {
            session
                .handle(key_message(sequence, revision, Key::Character(character)))
                .unwrap();
            sequence += 1;
            revision += 1;
        }
        (sequence, revision)
    }

    #[test]
    fn selecting_prefix_keeps_remaining_input_composing() {
        let mut session = Session::new(initial_header(), PartialPipeline);
        let (sequence, revision) = type_text(&mut session, "nihao", 1, 0);
        let output = session
            .handle(ui_message(
                sequence,
                revision,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        assert!(output.iter().all(|message| !matches!(
            message,
            EngineMessage::PlatformAction {
                action: PlatformAction {
                    kind: PlatformActionKind::Commit { .. },
                    ..
                },
                ..
            }
        )));
        assert_eq!(session.composition_text(), "旎hao");
        assert_eq!(session.active_input(), "hao");
    }

    #[test]
    fn selecting_final_segment_commits_the_composed_phrase() {
        let mut session = Session::new(initial_header(), PartialPipeline);
        let (sequence, revision) = type_text(&mut session, "nihao", 1, 0);
        session
            .handle(ui_message(
                sequence,
                revision,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        let output = session
            .handle(ui_message(
                sequence + 1,
                revision + 1,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision + 1),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        assert!(matches!(
            &output[0],
            EngineMessage::PlatformAction {
                action: PlatformAction {
                    kind: PlatformActionKind::Commit { text },
                    ..
                },
                ..
            } if text == "旎皓"
        ));
    }

    #[test]
    fn applied_novel_phrase_stages_learning() {
        let pipeline = RecordingLearningPipeline::default();
        let staged = Arc::clone(&pipeline.staged);
        let mut session = Session::new(initial_header(), pipeline);
        let (sequence, revision) = type_text(&mut session, "nihao", 1, 0);
        session
            .handle(ui_message(
                sequence,
                revision,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        let output = session
            .handle(ui_message(
                sequence + 1,
                revision + 1,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision + 1),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        let action_id = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => action.id,
            other => panic!("expected commit action, got {other:?}"),
        };
        session
            .handle(FrontendMessage::PlatformActionResult {
                header: {
                    let mut header = initial_header();
                    header.sequence = Sequence::new(sequence + 2);
                    header.revision = Revision::new(revision + 1);
                    header
                },
                result: PlatformActionResult {
                    action_id,
                    outcome: PlatformActionOutcome::Applied,
                },
            })
            .unwrap();
        let staged = staged.lock().unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].1.text, "旎皓");
        assert_eq!(staged[0].1.canonical_code, "ni hao");
        assert!(!staged[0].1.exact_phrase);
    }

    #[test]
    fn rejected_novel_phrase_does_not_stage_learning() {
        let pipeline = RecordingLearningPipeline::default();
        let staged = Arc::clone(&pipeline.staged);
        let mut session = Session::new(initial_header(), pipeline);
        let (sequence, revision) = type_text(&mut session, "nihao", 1, 0);
        session
            .handle(ui_message(
                sequence,
                revision,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        let output = session
            .handle(ui_message(
                sequence + 1,
                revision + 1,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision + 1),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        let action_id = match &output[0] {
            EngineMessage::PlatformAction { action, .. } => action.id,
            other => panic!("expected commit action, got {other:?}"),
        };
        session
            .handle(FrontendMessage::PlatformActionResult {
                header: {
                    let mut header = initial_header();
                    header.sequence = Sequence::new(sequence + 2);
                    header.revision = Revision::new(revision + 1);
                    header
                },
                result: PlatformActionResult {
                    action_id,
                    outcome: PlatformActionOutcome::Rejected {
                        reason: String::from("test"),
                    },
                },
            })
            .unwrap();
        assert!(staged.lock().unwrap().is_empty());
    }

    #[test]
    fn rollback_is_forwarded_idempotently() {
        let pipeline = RecordingLearningPipeline::default();
        let attempts = Arc::clone(&pipeline.cancel_attempts);
        let cancelled = Arc::clone(&pipeline.cancelled);
        let mut session = Session::new(initial_header(), pipeline);
        let token = cheime_model::CommitToken {
            session: SessionId::new(2),
            epoch: SessionEpoch::new(3),
            action_id: ActionId::new(1),
        };
        for sequence in [1, 2] {
            let mut header = initial_header();
            header.sequence = Sequence::new(sequence);
            session
                .handle(FrontendMessage::RollbackLearning { header, token })
                .unwrap();
        }
        assert_eq!(*attempts.lock().unwrap(), 2);
        assert_eq!(*cancelled.lock().unwrap(), vec![token]);
    }

    #[test]
    fn backspace_reopens_last_confirmed_segment_after_active_input_is_empty() {
        let mut session = Session::new(initial_header(), PartialPipeline);
        let (mut sequence, mut revision) = type_text(&mut session, "nihao", 1, 0);
        session
            .handle(ui_message(
                sequence,
                revision,
                UiCommand::SelectCandidate {
                    epoch: SessionEpoch::new(3),
                    snapshot_revision: Revision::new(revision),
                    candidate_id: CandidateId::new(1),
                },
            ))
            .unwrap();
        sequence += 1;
        revision += 1;
        for _ in 0..3 {
            session
                .handle(key_message(sequence, revision, Key::Backspace))
                .unwrap();
            sequence += 1;
            revision += 1;
        }
        session
            .handle(key_message(sequence, revision, Key::Backspace))
            .unwrap();
        assert_eq!(session.composition_text(), "ni");
        assert!(session.confirmed_segments().is_empty());
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
        // With no candidates, propose_commit falls back to cancel (not error)
        let mut session = Session::new(initial_header(), BuiltinPipeline::new([]));
        let result = session.handle(key_message(1, 0, Key::Enter));
        let msgs = result.unwrap();
        assert!(
            !msgs.is_empty(),
            "should return cancel action instead of error"
        );
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
