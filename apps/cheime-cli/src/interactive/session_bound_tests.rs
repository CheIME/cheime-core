use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    PlatformActionOutcome, PlatformActionResult, Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;
use chrono::{DateTime, Utc};

use super::SessionDriver;
use crate::interactive::log::RunId;

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

fn action_id(messages: &[EngineMessage]) -> cheime_model::ActionId {
    match &messages[0] {
        EngineMessage::PlatformAction { action, .. } => action.id,
        other => panic!("expected platform action, got {other:?}"),
    }
}

#[test]
fn open_and_close_emit_only_their_frontend_events() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-open-close"));

    // When
    let opened = driver
        .send_at(
            FrontendMessage::OpenSession {
                header: header(1, 0),
            },
            timestamp(),
        )
        .unwrap();
    let closed = driver
        .send_at(
            FrontendMessage::CloseSession {
                header: header(2, 0),
            },
            timestamp(),
        )
        .unwrap();

    // Then
    assert!(opened.messages.is_empty());
    assert_eq!(opened.events.len(), 1);
    assert!(closed.messages.is_empty());
    assert_eq!(closed.events.len(), 1);
}

#[test]
fn platform_action_result_can_emit_zero_or_one_engine_event() {
    // Given
    let entries = [("n".to_owned(), "en".to_owned(), 100)];
    let session = Session::new(header(0, 0), BuiltinPipeline::new(entries));
    let mut driver = SessionDriver::new(session, RunId::new("run-result-bounds"));
    let preedit = driver
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap();

    // When
    let zero_output = driver
        .send_at(
            FrontendMessage::PlatformActionResult {
                header: header(2, 1),
                result: PlatformActionResult {
                    action_id: action_id(&preedit.messages),
                    outcome: PlatformActionOutcome::Applied,
                },
            },
            timestamp(),
        )
        .unwrap();
    let commit = driver.send_at(key(3, 1, Key::Enter), timestamp()).unwrap();
    let one_output = driver
        .send_at(
            FrontendMessage::PlatformActionResult {
                header: header(4, 2),
                result: PlatformActionResult {
                    action_id: action_id(&commit.messages),
                    outcome: PlatformActionOutcome::Applied,
                },
            },
            timestamp(),
        )
        .unwrap();

    // Then
    assert!(zero_output.messages.is_empty());
    assert_eq!(zero_output.events.len(), 1);
    assert_eq!(one_output.messages.len(), 1);
    assert_eq!(one_output.events.len(), 2);
}
