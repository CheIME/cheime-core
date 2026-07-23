use cheime_model::{
    ActionId, CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent,
    KeyState, PlatformActionKind, PlatformActionOutcome, Revision, Sequence, SessionEpoch,
    SessionId, SessionStatus,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;
use chrono::{DateTime, Utc};

use super::SessionDriver;
use crate::interactive::{
    app::{AppState, PlatformActionApplication},
    log::{EventDirection, EventSequence, ProtocolEventPayload, RunId},
};

fn header(sequence: u64, revision: u64) -> MessageHeader {
    MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(10),
        session: SessionId::new(20),
        epoch: SessionEpoch::new(30),
        sequence: Sequence::new(sequence),
        revision: Revision::new(revision),
        deployment: DeploymentGeneration::new(40),
    }
}

fn key(sequence: u64, revision: u64, key: Key) -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: header(sequence, revision),
        event: KeyEvent {
            key,
            state: KeyState::default(),
        },
    }
}

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

#[test]
fn drains_commit_action_after_n_i_enter_and_inserts_text_once() {
    // Given
    let pipeline = BuiltinPipeline::new([
        (String::from("ni"), String::from("你"), 100),
        (String::from("ni"), String::from("呢"), 50),
    ]);
    let session = Session::new(header(0, 0), pipeline);
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-commit"));
    let mut state = AppState::new();
    let _ = driver
        .send_and_apply_at(key(1, 0, Key::Character('n')), timestamp(), &mut state)
        .unwrap();
    let _ = driver
        .send_and_apply_at(key(2, 1, Key::Character('i')), timestamp(), &mut state)
        .unwrap();

    // When
    let dispatch = driver
        .send_and_apply_at(key(3, 2, Key::Enter), timestamp(), &mut state)
        .unwrap();

    // Then
    assert_eq!(dispatch.messages.len(), 3);
    assert!(matches!(
        &dispatch.messages[0],
        EngineMessage::PlatformAction { action, .. }
            if action.id == ActionId::new(3)
                && action.kind == PlatformActionKind::Commit { text: "你".into() }
    ));
    assert!(matches!(
        &dispatch.messages[1],
        EngineMessage::CandidateSnapshot { snapshot, .. }
            if snapshot.status == SessionStatus::CommitPending && snapshot.preedit == "ni"
    ));
    assert!(matches!(
        &dispatch.messages[2],
        EngineMessage::CandidateSnapshot { snapshot, .. }
            if snapshot.status == SessionStatus::Ready && snapshot.preedit.is_empty()
    ));
    assert_eq!(
        dispatch.applications,
        vec![PlatformActionApplication::Committed {
            action: cheime_model::PlatformAction {
                id: ActionId::new(3),
                epoch: SessionEpoch::new(30),
                revision: Revision::new(2),
                kind: PlatformActionKind::Commit { text: "你".into() },
            },
            text: "你".into(),
        }]
    );
    assert_eq!(state.document().text(), "你");
    assert_eq!(state.document().cursor(), "你".len());
    assert!(matches!(
        state.snapshot(),
        Some(snapshot) if snapshot.status == SessionStatus::Ready && snapshot.preedit.is_empty()
    ));
    assert_eq!(dispatch.events.len(), 5);
    assert_eq!(
        dispatch
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(8),
            EventSequence::new(9),
            EventSequence::new(10),
            EventSequence::new(11),
            EventSequence::new(12),
        ]
    );
    assert_eq!(
        dispatch
            .events
            .iter()
            .map(|event| event.direction)
            .collect::<Vec<_>>(),
        vec![
            EventDirection::FrontendToEngine,
            EventDirection::EngineToFrontend,
            EventDirection::EngineToFrontend,
            EventDirection::FrontendToEngine,
            EventDirection::EngineToFrontend,
        ]
    );
    let action_header = match &dispatch.messages[0] {
        EngineMessage::PlatformAction { header, .. } => header,
        other => panic!("expected platform action, got {other:?}"),
    };
    assert!(matches!(
        &dispatch.events[3].payload,
        ProtocolEventPayload::Frontend(FrontendMessage::PlatformActionResult { header, result })
            if header == action_header
                && result.action_id == ActionId::new(3)
                && result.outcome == PlatformActionOutcome::Applied
    ));
}
