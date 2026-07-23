use super::{CliInternalEvent, EventSequence, ProtocolEvent, RunId};
use cheime_model::{
    ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState, Revision, Sequence,
    SessionEpoch, SessionId,
};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

fn header() -> MessageHeader {
    MessageHeader {
        protocol_version: 17,
        client: ClientInstanceId::new(23),
        session: SessionId::new(29),
        epoch: SessionEpoch::new(31),
        sequence: Sequence::new(37),
        revision: Revision::new(41),
        deployment: DeploymentGeneration::new(43),
    }
}

#[test]
fn frontend_event_when_serialized_preserves_context_name_and_complete_payload() {
    // Given
    let message = FrontendMessage::KeyCommand {
        header: header(),
        event: KeyEvent {
            key: Key::Character('Q'),
            state: KeyState {
                shift: true,
                control: false,
                alt: true,
            },
        },
    };

    // When
    let event = ProtocolEvent::frontend(
        timestamp(),
        RunId::new("run-alpha-47"),
        EventSequence::new(53),
        message,
    );
    let value = serde_json::to_value(event).unwrap();

    // Then
    assert_eq!(
        value,
        json!({
            "timestamp": "2031-02-03T04:05:06.789Z",
            "run_id": "run-alpha-47",
            "sequence": 53,
            "direction": "frontend_to_engine",
            "event_name": "frontend.key_command",
            "message_type": "KeyCommand",
            "payload": {
                "KeyCommand": {
                    "header": {
                        "protocol_version": 17,
                        "client": 23,
                        "session": 29,
                        "epoch": 31,
                        "sequence": 37,
                        "revision": 41,
                        "deployment": 43
                    },
                    "event": {
                        "key": { "Character": "Q" },
                        "state": { "shift": true, "control": false, "alt": true }
                    }
                }
            }
        })
    );
}

#[test]
fn protocol_event_constructors_are_infallible_typed_values() {
    // Given
    let message = FrontendMessage::OpenSession { header: header() };

    // When
    let event: ProtocolEvent = ProtocolEvent::frontend(
        timestamp(),
        RunId::new("run-infallible-59"),
        EventSequence::new(61),
        message,
    );

    // Then
    assert_eq!(event.message_type, "OpenSession");
}

#[test]
fn engine_event_when_serialized_derives_its_name_from_the_protocol_variant() {
    // Given
    let message = EngineMessage::ProtocolRejected {
        received: 59,
        supported: 61,
    };

    // When
    let event = ProtocolEvent::engine(
        timestamp(),
        RunId::new("run-bravo-67"),
        EventSequence::new(71),
        message,
    );
    let value = serde_json::to_value(event).unwrap();

    // Then
    assert_eq!(value["direction"], "engine_to_frontend");
    assert_eq!(value["event_name"], "engine.protocol_rejected");
    assert_eq!(value["message_type"], "ProtocolRejected");
    assert_eq!(
        value["payload"],
        json!({ "ProtocolRejected": { "received": 59, "supported": 61 } })
    );
}

#[test]
fn internal_event_when_serialized_uses_the_typed_internal_payload() {
    // Given
    let payload = CliInternalEvent::InputIgnored {
        reason: "control modifier".into(),
    };

    // When
    let event = ProtocolEvent::internal(
        timestamp(),
        RunId::new("run-charlie-73"),
        EventSequence::new(79),
        payload,
    );
    let value: Value = serde_json::to_value(event).unwrap();

    // Then
    assert_eq!(value["timestamp"], "2031-02-03T04:05:06.789Z");
    assert_eq!(value["run_id"], "run-charlie-73");
    assert_eq!(value["sequence"], 79);
    assert_eq!(value["direction"], "cli_internal");
    assert_eq!(value["event_name"], "cli.input_ignored");
    assert_eq!(value["message_type"], "InputIgnored");
    assert_eq!(
        value["payload"],
        json!({ "InputIgnored": { "reason": "control modifier" } })
    );
}
