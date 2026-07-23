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
    app::{AppState, LocalAction, PlatformActionApplication},
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
fn drains_set_preedit_action_and_acknowledges_without_engine_output() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-preedit"));
    let mut state = AppState::new();
    state.apply_local(LocalAction::Insert('原'));
    state.apply_local(LocalAction::Insert('文'));

    // When
    let dispatch = driver
        .send_and_apply_at(key(1, 0, Key::Character('n')), timestamp(), &mut state)
        .unwrap();

    // Then
    assert_eq!(dispatch.messages.len(), 2);
    assert!(matches!(
        &dispatch.messages[0],
        EngineMessage::PlatformAction { action, .. }
            if action.id == ActionId::new(1)
                && action.kind == PlatformActionKind::SetPreedit { text: "n".into(), cursor: 1 }
    ));
    assert_eq!(
        dispatch.applications,
        vec![PlatformActionApplication::NoDocumentChange {
            action: cheime_model::PlatformAction {
                id: ActionId::new(1),
                epoch: SessionEpoch::new(30),
                revision: Revision::new(1),
                kind: PlatformActionKind::SetPreedit {
                    text: "n".into(),
                    cursor: 1
                },
            },
        }]
    );
    assert_eq!(state.document().text(), "原文");
    assert_eq!(state.document().cursor(), "原文".len());
    assert!(matches!(
        state.snapshot(),
        Some(snapshot) if snapshot.status == SessionStatus::Composing && snapshot.preedit == "n"
    ));
    assert_eq!(dispatch.events.len(), 4);
    assert_eq!(
        dispatch
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(0),
            EventSequence::new(1),
            EventSequence::new(2),
            EventSequence::new(3),
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
                && result.action_id == ActionId::new(1)
                && result.outcome == PlatformActionOutcome::Applied
    ));
}

#[test]
fn drains_cancel_composition_action_and_applies_ready_clear() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-cancel"));
    let mut state = AppState::new();
    state.apply_local(LocalAction::Insert('原'));
    state.apply_local(LocalAction::Insert('文'));
    let _ = driver
        .send_and_apply_at(key(1, 0, Key::Character('n')), timestamp(), &mut state)
        .unwrap();

    // When
    let dispatch = driver
        .send_and_apply_at(key(2, 1, Key::Escape), timestamp(), &mut state)
        .unwrap();

    // Then
    assert_eq!(dispatch.messages.len(), 2);
    assert!(matches!(
        &dispatch.messages[0],
        EngineMessage::PlatformAction { action, .. }
            if action.id == ActionId::new(2) && action.kind == PlatformActionKind::CancelComposition
    ));
    assert!(matches!(
        &dispatch.messages[1],
        EngineMessage::CandidateSnapshot { snapshot, .. }
            if snapshot.status == SessionStatus::Ready && snapshot.preedit.is_empty()
    ));
    assert_eq!(
        dispatch.applications,
        vec![PlatformActionApplication::NoDocumentChange {
            action: cheime_model::PlatformAction {
                id: ActionId::new(2),
                epoch: SessionEpoch::new(30),
                revision: Revision::new(1),
                kind: PlatformActionKind::CancelComposition,
            },
        }]
    );
    assert_eq!(state.document().text(), "原文");
    assert_eq!(state.document().cursor(), "原文".len());
    assert!(matches!(
        state.snapshot(),
        Some(snapshot) if snapshot.status == SessionStatus::Ready && snapshot.preedit.is_empty()
    ));
    assert_eq!(
        dispatch
            .events
            .iter()
            .map(|event| event.direction)
            .collect::<Vec<_>>(),
        vec![
            EventDirection::FrontendToEngine,
            EventDirection::EngineToFrontend,
            EventDirection::FrontendToEngine,
            EventDirection::EngineToFrontend,
        ]
    );
}

#[test]
fn drains_many_platform_actions_in_sequence_with_strictly_increasing_events() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-many"));
    let mut state = AppState::new();
    let mut events = Vec::new();
    let mut applications = Vec::new();

    // When
    for sequence in 1..=32 {
        let dispatch = driver
            .send_and_apply_at(
                key(sequence, sequence - 1, Key::Character('n')),
                timestamp(),
                &mut state,
            )
            .unwrap();
        events.extend(dispatch.events);
        applications.extend(dispatch.applications);
    }

    // Then
    assert_eq!(applications.len(), 32);
    assert_eq!(
        applications
            .iter()
            .map(|application| match application {
                PlatformActionApplication::NoDocumentChange { action }
                | PlatformActionApplication::Committed { action, .. } => action.id,
            })
            .collect::<Vec<_>>(),
        (1..=32).map(ActionId::new).collect::<Vec<_>>()
    );
    assert_eq!(events.len(), 128);
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        (0..128).map(EventSequence::new).collect::<Vec<_>>()
    );
    assert!(events.iter().step_by(4).all(|event| matches!(
        &event.payload,
        ProtocolEventPayload::Frontend(FrontendMessage::KeyCommand { .. })
    )));
    assert!(events.iter().skip(3).step_by(4).all(|event| matches!(
        &event.payload,
        ProtocolEventPayload::Frontend(FrontendMessage::PlatformActionResult { result, .. })
            if result.outcome == PlatformActionOutcome::Applied
    )));
    assert_eq!(state.document().text(), "");
    assert_eq!(state.document().cursor(), 0);
    assert!(matches!(
        state.snapshot(),
        Some(snapshot) if snapshot.status == SessionStatus::Composing && snapshot.preedit.len() == 32
    ));
}
