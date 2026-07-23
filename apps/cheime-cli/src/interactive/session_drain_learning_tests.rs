use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::{
    BuiltinPipeline, InputPipeline, PipelineError, PipelineIntent, PipelineUpdate,
};
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::Session;
use cheime_user_data::UserStore;
use chrono::{DateTime, Utc};

use super::{SessionApplicationContext, SessionDispatchError, SessionDriver};
use crate::interactive::{
    app::{AppState, PlatformActionApplication},
    log::{EventSequence, RunId},
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
fn stages_committed_text_once_after_applied_acknowledgement() {
    // Given
    let pipeline = BuiltinPipeline::new([(String::from("ni"), String::from("你"), 100)]);
    let session = Session::new(header(0, 0), pipeline);
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-learning-commit"));
    let mut state = AppState::new();
    let mut store = UserStore::new("learning-commit");
    let _ = driver
        .send_and_apply_at(
            key(1, 0, Key::Character('n')),
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();
    let _ = driver
        .send_and_apply_at(
            key(2, 1, Key::Character('i')),
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();
    store.confirm_all_pending();
    assert_eq!(store.frequency("quanpin", "你"), 0);

    // When
    let dispatch = driver
        .send_and_apply_at(
            key(3, 2, Key::Enter),
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();

    // Then
    assert!(matches!(
        dispatch.applications.as_slice(),
        [PlatformActionApplication::Committed { text, .. }] if text == "你"
    ));
    store.confirm_all_pending();
    assert_eq!(store.frequency("quanpin", "你"), 1);
}

#[test]
fn does_not_stage_set_preedit_learning() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-learning-preedit"));
    let mut state = AppState::new();
    let mut store = UserStore::new("learning-preedit");

    // When
    let dispatch = driver
        .send_and_apply_at(
            key(1, 0, Key::Character('n')),
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();

    // Then
    assert!(matches!(
        dispatch.applications.as_slice(),
        [PlatformActionApplication::NoDocumentChange { .. }]
    ));
    store.confirm_all_pending();
    assert_eq!(store.frequency("quanpin", "n"), 0);
}

#[test]
fn does_not_stage_cancel_composition_learning() {
    // Given
    let session = Session::new(header(0, 0), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-learning-cancel"));
    let mut state = AppState::new();
    let mut store = UserStore::new("learning-cancel");
    let _ = driver
        .send_and_apply_at(
            key(1, 0, Key::Character('n')),
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();

    // When
    let dispatch = driver
        .send_and_apply_at(
            key(2, 1, Key::Escape),
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();

    // Then
    assert!(matches!(
        dispatch.applications.as_slice(),
        [PlatformActionApplication::NoDocumentChange { .. }]
    ));
    store.confirm_all_pending();
    assert_eq!(store.frequency("quanpin", "n"), 0);
}

#[test]
fn does_not_stage_commit_learning_when_acknowledgement_dispatch_fails() {
    // Given
    let session = Session::new(header(0, 0), CommitOnKeyPipeline);
    let mut driver = SessionDriver::new(session, RunId::new("run-drain-learning-failure"))
        .with_initial_sequence(EventSequence::new(u64::MAX - 2));
    let mut state = AppState::new();
    let mut store = UserStore::new("learning-failure");

    // When
    let result = driver.send_and_apply_at(
        key(1, 0, Key::Character('n')),
        timestamp(),
        SessionApplicationContext::new(&mut state, &mut store),
    );

    // Then
    assert!(matches!(
        result,
        Err(SessionDispatchError::SequenceOverflow)
    ));
    assert_eq!(state.document().text(), "unconfirmed");
    store.confirm_all_pending();
    assert_eq!(store.frequency("quanpin", "unconfirmed"), 0);
}

struct CommitOnKeyPipeline;

impl InputPipeline for CommitOnKeyPipeline {
    fn apply(&self, composition: &str, _event: &KeyEvent) -> Result<PipelineUpdate, PipelineError> {
        Ok(PipelineUpdate {
            composition: composition.to_owned(),
            candidates: Vec::new(),
            intent: PipelineIntent::CommitText(String::from("unconfirmed")),
        })
    }
}
