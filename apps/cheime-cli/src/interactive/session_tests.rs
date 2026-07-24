use super::{SessionDriver, drain::normalized_pinyin};
use crate::interactive::app::AppState;
use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId, SessionStatus,
};
use cheime_pipeline::{
    BuiltinPipeline, InputPipeline, PipelineError, PipelineIntent, PipelineUpdate,
};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::{Session, SessionError};
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::sync::Arc;

fn header(sequence: u64) -> MessageHeader {
    MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(1),
        epoch: SessionEpoch::new(1),
        sequence: Sequence::new(sequence),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(1),
    }
}

fn key(sequence: u64, key: Key) -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: header(sequence),
        event: KeyEvent {
            key,
            state: KeyState::default(),
        },
    }
}

fn driver() -> SessionDriver<BuiltinPipeline> {
    let pipeline = BuiltinPipeline::new([(String::from("ni"), String::from("你"), 100)]);
    SessionDriver::new(Session::new(header(0), pipeline))
}

#[test]
fn key_updates_snapshot_without_modifying_document() {
    let mut driver = driver();
    let mut state = AppState::new();
    let store = Arc::new(Mutex::new(UserStore::new("test")));

    let messages = driver
        .send_and_apply(key(1, Key::Character('n')), &mut state, &store)
        .unwrap();

    assert_eq!(state.document().text(), "");
    assert_eq!(state.snapshot().unwrap().preedit, "n");
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, EngineMessage::PlatformAction { .. }))
    );
}

#[test]
fn commit_is_applied_acknowledged_and_learned_once() {
    let mut driver = driver();
    let mut state = AppState::new();
    let store = Arc::new(Mutex::new(UserStore::new("test")));

    for (sequence, character) in [(1, 'n'), (2, 'i')] {
        driver
            .send_and_apply(key(sequence, Key::Character(character)), &mut state, &store)
            .unwrap();
    }
    let messages = driver
        .send_and_apply(key(3, Key::Enter), &mut state, &store)
        .unwrap();

    assert_eq!(state.document().text(), "你");
    assert_eq!(state.snapshot().unwrap().status, SessionStatus::Ready);
    assert!(messages.iter().any(|message| {
        matches!(
            message,
            EngineMessage::CandidateSnapshot { snapshot, .. }
                if snapshot.status == SessionStatus::CommitPending
        )
    }));
    driver.finish_learning(&store);
    let store = store.lock();
    assert_eq!(store.frequency("quanpin", "你"), 1);
    assert_eq!(store.query("ni")[0].text, "你");
}

#[test]
fn stale_sequence_is_returned_directly() {
    let mut driver = driver();
    let mut state = AppState::new();
    let store = Arc::new(Mutex::new(UserStore::new("test")));
    driver
        .send_and_apply(key(1, Key::Character('n')), &mut state, &store)
        .unwrap();

    let result = driver.send_and_apply(key(1, Key::Character('i')), &mut state, &store);

    assert!(matches!(
        result,
        Err(SessionError::StaleSequence { received, last })
            if received == Sequence::new(1) && last == Sequence::new(1)
    ));
}

#[test]
fn pipeline_can_lock_shared_user_store_during_dispatch() {
    let store = Arc::new(Mutex::new(UserStore::new("shared")));
    let pipeline = LockCheckingPipeline {
        store: Arc::clone(&store),
    };
    let mut driver = SessionDriver::new(Session::new(header(0), pipeline));
    let mut state = AppState::new();

    driver
        .send_and_apply(key(1, Key::Character('n')), &mut state, &store)
        .unwrap();
}

#[test]
fn learning_code_matches_segmented_user_dictionary_lookup() {
    assert_eq!(normalized_pinyin("zhongguo"), "zhong guo");
}

struct LockCheckingPipeline {
    store: Arc<Mutex<UserStore>>,
}

impl InputPipeline for LockCheckingPipeline {
    fn apply(&self, composition: &str, _event: &KeyEvent) -> Result<PipelineUpdate, PipelineError> {
        let _store = self
            .store
            .try_lock()
            .expect("pipeline must run without an outer user-store lock");
        Ok(PipelineUpdate {
            composition: composition.to_owned(),
            candidates: Vec::new(),
            intent: PipelineIntent::None,
        })
    }
}
