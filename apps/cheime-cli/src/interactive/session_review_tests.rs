use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::{
    BuiltinPipeline, InputPipeline, PipelineError, PipelineIntent, PipelineUpdate,
};
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::{Session, SessionError};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;

use super::{SessionDispatchError, SessionDriver};
use crate::interactive::log::{EventSequence, RunId};

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
fn production_constructor_uses_supplied_run_id_for_every_event() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-supplied"));

    // When
    let dispatch = driver
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap();

    // Then
    assert!(
        dispatch
            .events
            .iter()
            .all(|event| event.run_id == RunId::new("run-supplied"))
    );
}

#[test]
fn stale_session_error_carries_frontend_event_and_consumes_one_sequence() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-stale"));
    let _ = driver
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap();

    // When
    let error = driver
        .send_at(key(1, 1, Key::Character('i')), timestamp())
        .unwrap_err();
    let following = driver
        .send_at(key(2, 1, Key::Character('i')), timestamp())
        .unwrap();

    // Then
    assert!(matches!(
        error,
        SessionDispatchError::Session { error: SessionError::StaleSequence { received, last }, frontend_event }
            if received == Sequence::new(1)
                && last == Sequence::new(1)
                && frontend_event.sequence == EventSequence::new(3)
    ));
    assert_eq!(
        following
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

#[test]
fn preflight_overflow_keeps_sequence_and_session_unmutated() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-overflow"))
        .with_initial_sequence(EventSequence::new(u64::MAX - 1));

    // When
    let overflow = driver.send_at(key(1, 0, Key::Character('n')), timestamp());
    let after_overflow = driver
        .send_at(
            FrontendMessage::OpenSession {
                header: header(1, 0),
            },
            timestamp(),
        )
        .unwrap();
    let fresh_session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut exact_capacity = SessionDriver::new(fresh_session, RunId::new("run-exact"))
        .with_initial_sequence(EventSequence::new(u64::MAX - 3));
    let exact_dispatch = exact_capacity
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap();

    // Then
    assert!(matches!(
        overflow,
        Err(SessionDispatchError::SequenceOverflow)
    ));
    assert_eq!(after_overflow.events.len(), 1);
    assert_eq!(
        after_overflow.events[0].sequence,
        EventSequence::new(u64::MAX - 1)
    );
    assert_eq!(
        exact_dispatch
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(u64::MAX - 3),
            EventSequence::new(u64::MAX - 2),
            EventSequence::new(u64::MAX - 1),
        ]
    );
}

#[test]
fn accepted_header_pipeline_error_carries_frontend_event_and_advances_session_sequence() {
    // Given
    let session = Session::new(header(0, 0), FailOncePipeline::new());
    let mut driver = SessionDriver::new(session, RunId::new("run-pipeline"));

    // When
    let error = driver
        .send_at(key(1, 0, Key::Character('n')), timestamp())
        .unwrap_err();
    let following = driver
        .send_at(key(2, 0, Key::Character('i')), timestamp())
        .unwrap();

    // Then
    assert!(matches!(
        error,
        SessionDispatchError::Session {
            error: SessionError::Pipeline(PipelineError::UnsupportedCharacter('x')),
            frontend_event,
        } if frontend_event.sequence == EventSequence::new(0)
    ));
    assert_eq!(following.messages.len(), 2);
    assert_eq!(
        following
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(1),
            EventSequence::new(2),
            EventSequence::new(3),
        ]
    );
}

struct FailOncePipeline {
    should_fail: Mutex<bool>,
}

impl FailOncePipeline {
    fn new() -> Self {
        Self {
            should_fail: Mutex::new(true),
        }
    }
}

impl InputPipeline for FailOncePipeline {
    fn apply(&self, composition: &str, _event: &KeyEvent) -> Result<PipelineUpdate, PipelineError> {
        let mut should_fail = self.should_fail.lock();
        if *should_fail {
            *should_fail = false;
            return Err(PipelineError::UnsupportedCharacter('x'));
        }
        Ok(PipelineUpdate {
            composition: composition.to_owned(),
            candidates: Vec::new(),
            intent: PipelineIntent::None,
        })
    }
}
