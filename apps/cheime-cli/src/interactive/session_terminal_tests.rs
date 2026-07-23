use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::Session;
use chrono::{DateTime, Utc};

use super::{SessionDispatchError, SessionDriver};
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

fn key(sequence: u64) -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: header(sequence, 0),
        event: KeyEvent {
            key: Key::Character('n'),
            state: KeyState::default(),
        },
    }
}

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

#[test]
fn output_contract_violation_poison_driver_permanently() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver =
        SessionDriver::new(session, RunId::new("run-terminal")).with_output_bound_override(0);

    // When
    let violation = driver.send_at(key(1), timestamp());
    let after_violation = driver.send_at(key(2), timestamp());

    // Then
    assert!(matches!(
        violation,
        Err(SessionDispatchError::OutputContractViolation {
            maximum: 0,
            actual: 2,
        })
    ));
    assert!(matches!(
        after_violation,
        Err(SessionDispatchError::DriverPoisoned)
    ));
}
