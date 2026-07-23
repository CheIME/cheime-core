use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::{Session, SessionError};
use chrono::{DateTime, Utc};
use serde_json::json;

use super::{SessionDispatchError, SessionDriver};
use crate::interactive::log::{EventDirection, EventSequence, RunId};

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
fn send_at_returns_frontend_set_preedit_and_candidate_events_in_order() {
    // Given
    let entries = [("n".to_owned(), "en".to_owned(), 100)];
    let expected_session = Session::new(header(0, 0), BuiltinPipeline::new(entries.clone()));
    let session = Session::new(header(0, 0), BuiltinPipeline::new(entries));
    let mut expected_session = expected_session;
    let mut driver = SessionDriver::new(session, RunId::new("run-alpha"));
    let message = key(1, 0, Key::Character('n'));

    // When
    let expected_messages = expected_session.handle(message.clone()).unwrap();
    let dispatch = driver.send_at(message, timestamp()).unwrap();

    // Then
    assert_eq!(dispatch.messages, expected_messages);
    assert_eq!(dispatch.events.len(), 3);
    assert_eq!(
        dispatch
            .events
            .iter()
            .map(|event| (event.direction, event.event_name, event.message_type))
            .collect::<Vec<_>>(),
        vec![
            (
                EventDirection::FrontendToEngine,
                "frontend.key_command",
                "KeyCommand",
            ),
            (
                EventDirection::EngineToFrontend,
                "engine.platform_action",
                "PlatformAction",
            ),
            (
                EventDirection::EngineToFrontend,
                "engine.candidate_snapshot",
                "CandidateSnapshot",
            ),
        ]
    );
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
        ]
    );
    assert!(
        dispatch
            .events
            .iter()
            .all(|event| event.timestamp == timestamp())
    );
    assert!(
        dispatch
            .events
            .iter()
            .all(|event| event.run_id == RunId::new("run-alpha"))
    );
    assert_eq!(
        serde_json::to_value(&dispatch.events[0].payload).unwrap(),
        json!({
            "KeyCommand": {
                "header": {
                    "protocol_version": CORE_PROTOCOL_VERSION,
                    "client": 10,
                    "session": 20,
                    "epoch": 30,
                    "sequence": 1,
                    "revision": 0,
                    "deployment": 40,
                },
                "event": {
                    "key": { "Character": "n" },
                    "state": { "shift": false, "control": false, "alt": false },
                },
            },
        })
    );
    assert_eq!(
        serde_json::to_value(&dispatch.events[1].payload).unwrap(),
        serde_json::to_value(&dispatch.messages[0]).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&dispatch.events[1].payload).unwrap()["PlatformAction"]["action"],
        json!({
            "id": 1,
            "epoch": 30,
            "revision": 1,
            "kind": { "SetPreedit": { "text": "n", "cursor": 1 } },
        })
    );
    assert_eq!(
        serde_json::to_value(&dispatch.events[2].payload).unwrap(),
        serde_json::to_value(&dispatch.messages[1]).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&dispatch.events[2].payload).unwrap()["CandidateSnapshot"]["snapshot"]
            ["preedit"],
        "n"
    );
    assert_eq!(
        serde_json::to_value(&dispatch.events[2].payload).unwrap()["CandidateSnapshot"]["snapshot"]
            ["candidates"],
        json!([{
            "id": 1,
            "text": "en",
            "annotation": "n",
            "source": "builtin",
            "is_emoji": false,
        }])
    );
}

#[test]
fn send_at_continues_event_sequences_across_dispatches() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-bravo"));

    // When
    let first = driver
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap();
    let second = driver
        .send_at(key(2, 1, Key::Character('i')), timestamp())
        .unwrap();

    // Then
    assert_eq!(
        first
            .events
            .iter()
            .chain(&second.events)
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(0),
            EventSequence::new(1),
            EventSequence::new(2),
            EventSequence::new(3),
            EventSequence::new(4),
            EventSequence::new(5),
        ]
    );
}

#[test]
fn send_at_keeps_sequences_and_engine_events_absent_after_stale_session_error() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-charlie"));
    let first = driver
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap();

    // When
    let error = driver
        .send_at(key(1, 1, Key::Character('i')), timestamp())
        .unwrap_err();
    let after_error = driver
        .send_at(key(2, 1, Key::Character('i')), timestamp())
        .unwrap();

    // Then
    assert!(matches!(
        error,
        SessionDispatchError::Session { error: SessionError::StaleSequence {
            received,
            last,
        }, .. } if received == Sequence::new(1) && last == Sequence::new(1)
    ));
    assert_eq!(first.events.len(), 3);
    assert_eq!(after_error.events.len(), 3);
    assert_eq!(
        after_error
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(4),
            EventSequence::new(5),
            EventSequence::new(6),
        ]
    );
}
