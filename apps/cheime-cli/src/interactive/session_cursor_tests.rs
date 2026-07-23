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
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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

fn key(sequence: u64, revision: u64) -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: header(sequence, revision),
        event: KeyEvent {
            key: Key::Character('n'),
            state: KeyState::default(),
        },
    }
}

fn open(sequence: u64, revision: u64) -> FrontendMessage {
    FrontendMessage::OpenSession {
        header: header(sequence, revision),
    }
}

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

#[test]
fn open_at_max_assigns_final_sequence_then_exhausts_driver() {
    // Given
    let applications = Arc::new(AtomicUsize::new(0));
    let session = Session::new(header(0, 0), CountingPipeline::new(applications.clone()));
    let mut driver = SessionDriver::new(session, RunId::new("run-open-max"))
        .with_initial_sequence(EventSequence::new(u64::MAX));

    // When
    let final_event = driver.send_at(open(1, 0), timestamp()).unwrap();
    let after_exhaustion = driver.send_at(key(2, 0), timestamp());

    // Then
    assert_eq!(final_event.events.len(), 1);
    assert_eq!(final_event.events[0].sequence, EventSequence::new(u64::MAX));
    assert!(matches!(
        after_exhaustion,
        Err(SessionDispatchError::SequenceOverflow)
    ));
    assert_eq!(applications.load(Ordering::Relaxed), 0);
}

struct CountingPipeline {
    applications: Arc<AtomicUsize>,
}

impl CountingPipeline {
    fn new(applications: Arc<AtomicUsize>) -> Self {
        Self { applications }
    }
}

impl InputPipeline for CountingPipeline {
    fn apply(&self, composition: &str, _event: &KeyEvent) -> Result<PipelineUpdate, PipelineError> {
        self.applications.fetch_add(1, Ordering::Relaxed);
        Ok(PipelineUpdate {
            composition: composition.to_owned(),
            candidates: Vec::new(),
            intent: PipelineIntent::None,
        })
    }
}

#[test]
fn stale_open_at_max_carries_final_frontend_event_then_exhausts_driver() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-stale-max"))
        .with_initial_sequence(EventSequence::new(u64::MAX));

    // When
    let error = driver.send_at(open(1, 1), timestamp()).unwrap_err();
    let after_exhaustion = driver.send_at(open(2, 0), timestamp());

    // Then
    assert!(matches!(
        error,
        SessionDispatchError::Session {
            error: SessionError::StaleRevision { received, current },
            frontend_event,
        } if received == Revision::new(1)
            && current == Revision::new(0)
            && frontend_event.sequence == EventSequence::new(u64::MAX)
    ));
    assert!(matches!(
        after_exhaustion,
        Err(SessionDispatchError::SequenceOverflow)
    ));
}

#[test]
fn key_at_max_minus_two_assigns_three_events_then_exhausts_driver() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-key-max"))
        .with_initial_sequence(EventSequence::new(u64::MAX - 2));

    // When
    let dispatch = driver.send_at(key(1, 0), timestamp()).unwrap();
    let after_exhaustion = driver.send_at(open(2, 1), timestamp());

    // Then
    assert_eq!(
        dispatch
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(u64::MAX - 2),
            EventSequence::new(u64::MAX - 1),
            EventSequence::new(u64::MAX),
        ]
    );
    assert!(matches!(
        after_exhaustion,
        Err(SessionDispatchError::SequenceOverflow)
    ));
}
