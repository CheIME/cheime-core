use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    PlatformActionKind, Revision, Sequence, SessionEpoch, SessionId, SessionStatus,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::{Session, SessionError};

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

#[test]
fn sends_key_command_messages_in_session_order() {
    // Given
    let entries = [("n".to_owned(), "en".to_owned(), 100)];
    let expected_session = Session::new(header(0, 0), BuiltinPipeline::new(entries.clone()));
    let session = Session::new(header(0, 0), BuiltinPipeline::new(entries));
    let mut expected_session = expected_session;
    let mut driver = SessionDriver::new(session, RunId::new("run-legacy"));
    let message = key(1, 0, Key::Character('n'));

    // When
    let expected_messages = expected_session.handle(message.clone()).unwrap();
    let messages = driver
        .send_at(message, "2031-02-03T04:05:06.789Z".parse().unwrap())
        .unwrap()
        .messages;

    // Then
    assert_eq!(messages, expected_messages);
    assert_eq!(messages.len(), 2);
    assert!(matches!(
        &messages[0],
        EngineMessage::PlatformAction { action, .. }
            if matches!(
                &action.kind,
                PlatformActionKind::SetPreedit { text, cursor }
                    if text == "n" && *cursor == 1
            )
    ));
    assert!(matches!(
        &messages[1],
        EngineMessage::CandidateSnapshot { snapshot, .. }
            if snapshot.preedit == "n" && snapshot.status == SessionStatus::Composing
    ));
}

#[test]
fn returns_stale_sequence_session_error() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-legacy"));
    let _ = driver
        .send_at(
            key(1, 0, Key::Character('n')),
            "2031-02-03T04:05:06.789Z".parse().unwrap(),
        )
        .unwrap();

    // When
    let result = driver.send_at(
        key(1, 1, Key::Character('i')),
        "2031-02-03T04:05:06.789Z".parse().unwrap(),
    );

    // Then
    assert!(matches!(
        result,
        Err(super::SessionDispatchError::Session {
            error: SessionError::StaleSequence {
                received,
                last,
            },
            ..
        }) if received == Sequence::new(1) && last == Sequence::new(1)
    ));
}
