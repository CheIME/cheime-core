use crate::SessionError;
use cheime_model::{
    ActionId, CandidateSnapshot, PlatformAction, PlatformActionKind, PlatformActionOutcome,
    Revision, SessionStatus,
};
use cheime_pipeline::{InputPipeline, PipelineIntent};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
enum PendingEffect {
    ClearComposition,
    KeepComposition,
}

pub struct Session<P> {
    identity: MessageHeader,
    pipeline: P,
    composition: String,
    candidates: Vec<cheime_model::Candidate>,
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
            next_action: 1,
            pending: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn composition(&self) -> &str {
        &self.composition
    }

    pub fn handle(&mut self, message: FrontendMessage) -> Result<Vec<EngineMessage>, SessionError> {
        self.validate_header(message.header())?;
        self.last_sequence = message.header().sequence;

        match message {
            FrontendMessage::KeyCommand { event, .. } => self.handle_key(event),
            FrontendMessage::PlatformActionResult { result, .. } => {
                self.handle_action_result(result)
            }
            FrontendMessage::UiCommand { .. }
            | FrontendMessage::OpenSession { .. }
            | FrontendMessage::CloseSession { .. } => Ok(Vec::new()),
        }
    }

    fn validate_header(&self, header: &MessageHeader) -> Result<(), SessionError> {
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
        if header.revision != self.revision {
            return Err(SessionError::StaleRevision {
                received: header.revision,
                current: self.revision,
            });
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
            PipelineIntent::None => {
                self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
                self.composition = update.composition;
                self.candidates = update.candidates;
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

    fn propose_commit(&mut self) -> Result<Vec<EngineMessage>, SessionError> {
        let text = self
            .candidates
            .first()
            .map(|candidate| candidate.text.clone())
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
            self.revision = self.revision.next().ok_or(SessionError::RevisionOverflow)?;
            return Ok(vec![self.snapshot_message(SessionStatus::Ready)]);
        }
        Ok(Vec::new())
    }

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
                candidates: self.candidates.clone(),
                highlighted: self.candidates.first().map(|candidate| candidate.id),
                status,
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
        BuiltinPipeline::new([(String::from("ni"), String::from("你"), 10)])
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
}
