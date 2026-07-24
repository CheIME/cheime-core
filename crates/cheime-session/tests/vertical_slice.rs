use cheime_dictionary::{CompiledIndex, DictEntry};
use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    PlatformActionKind, PlatformActionOutcome, PlatformActionResult, Revision, Sequence,
    SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_pipeline::ComposablePipeline;
use cheime_pipeline::learning::{FakeClock, LEARNING_DELAY_MS, LearningService};
use cheime_pipeline::processor::DefaultProcessor;
use cheime_pipeline::ranker::UnifiedRanker;
use cheime_pipeline::segmentor::PinyinSegmentor;
use cheime_pipeline::translator::{DictTranslator, UserDictTranslator};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::sync::Arc;

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
fn frontend_commands_produce_confirmed_commit() {
    let pipeline = BuiltinPipeline::new([
        (String::from("ni"), String::from("你"), 100),
        (String::from("ni"), String::from("呢"), 50),
    ]);
    let mut session = Session::new(header(0, 0), pipeline);

    session.handle(key(1, 0, Key::Character('n'))).unwrap();
    let second = session.handle(key(2, 1, Key::Character('i'))).unwrap();
    assert!(matches!(
        &second[1],
        EngineMessage::CandidateSnapshot { snapshot, .. }
            if snapshot.preedit == "ni"
                && snapshot.candidates[0].text == "你"
                && snapshot.revision == Revision::new(2)
    ));

    let commit = session.handle(key(3, 2, Key::Enter)).unwrap();
    let action = match &commit[0] {
        EngineMessage::PlatformAction { action, .. } => action,
        other => panic!("expected commit action, got {other:?}"),
    };
    assert_eq!(
        action.kind,
        PlatformActionKind::Commit {
            text: String::from("你")
        }
    );
    assert_eq!(session.composition(), "ni");

    let confirmation = session
        .handle(FrontendMessage::PlatformActionResult {
            header: header(4, 2),
            result: PlatformActionResult {
                action_id: action.id,
                outcome: PlatformActionOutcome::Applied,
            },
        })
        .unwrap();
    assert!(matches!(
        &confirmation[0],
        EngineMessage::CandidateSnapshot { snapshot, .. }
            if snapshot.preedit.is_empty() && snapshot.revision == Revision::new(3)
    ));
    assert_eq!(session.composition(), "");
}

#[test]
fn constructed_phrase_is_learned_after_delayed_confirmation() {
    let index = Arc::new(CompiledIndex::build(
        vec![
            DictEntry {
                text: "旎".into(),
                code: "ni".into(),
                weight: Some(100),
                stem: None,
            },
            DictEntry {
                text: "皓".into(),
                code: "hao".into(),
                weight: Some(100),
                stem: None,
            },
        ],
        DeploymentGeneration::new(40),
    ));
    let store = Arc::new(Mutex::new(UserStore::new("vertical-slice")));
    let clock = Arc::new(FakeClock::new(0));
    let learning = Arc::new(LearningService::new(store.clone(), clock.clone()));
    let pipeline = ComposablePipeline::new(
        Box::new(DefaultProcessor::new()),
        Box::new(PinyinSegmentor::new()),
        None,
        vec![
            Box::new(UserDictTranslator::new(store.clone())),
            Box::new(DictTranslator::new("inline", index)),
        ],
        vec![],
        Box::new(UnifiedRanker::new(Default::default())),
    )
    .with_schema_id("vertical-slice")
    .with_learning(learning.clone());
    let mut session = Session::new(header(0, 0), pipeline);
    let mut sequence = 1;
    let mut revision = 0;

    let mut snapshot = None;
    for character in "nihao".chars() {
        let output = session
            .handle(key(sequence, revision, Key::Character(character)))
            .unwrap();
        sequence += 1;
        snapshot = output.into_iter().find_map(|message| match message {
            EngineMessage::CandidateSnapshot { snapshot, .. } => Some(snapshot),
            _ => None,
        });
        revision = snapshot.as_ref().unwrap().revision.get();
    }

    let first = snapshot
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .find(|candidate| candidate.text == "旎")
        .unwrap()
        .id;
    let output = session
        .handle(FrontendMessage::UiCommand {
            header: header(sequence, revision),
            command: cheime_model::UiCommand::SelectCandidate {
                epoch: SessionEpoch::new(30),
                snapshot_revision: Revision::new(revision),
                candidate_id: first,
            },
        })
        .unwrap();
    sequence += 1;
    let snapshot = output
        .into_iter()
        .find_map(|message| match message {
            EngineMessage::CandidateSnapshot { snapshot, .. } => Some(snapshot),
            _ => None,
        })
        .unwrap();
    revision = snapshot.revision.get();
    assert_eq!(snapshot.preedit, "旎hao");

    let second = snapshot
        .candidates
        .iter()
        .find(|candidate| candidate.text == "皓")
        .unwrap()
        .id;
    let output = session
        .handle(FrontendMessage::UiCommand {
            header: header(sequence, revision),
            command: cheime_model::UiCommand::SelectCandidate {
                epoch: SessionEpoch::new(30),
                snapshot_revision: Revision::new(revision),
                candidate_id: second,
            },
        })
        .unwrap();
    sequence += 1;
    let action = output
        .iter()
        .find_map(|message| match message {
            EngineMessage::PlatformAction { action, .. } => Some(action.clone()),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        &action.kind,
        PlatformActionKind::Commit { text } if text == "旎皓"
    ));

    let output = session
        .handle(FrontendMessage::PlatformActionResult {
            header: header(sequence, revision),
            result: PlatformActionResult {
                action_id: action.id,
                outcome: PlatformActionOutcome::Applied,
            },
        })
        .unwrap();
    sequence += 1;
    revision = output
        .iter()
        .find_map(|message| match message {
            EngineMessage::CandidateSnapshot { snapshot, .. } => Some(snapshot.revision.get()),
            _ => None,
        })
        .unwrap();
    assert!(store.lock().query("ni hao").is_empty());

    clock.set(LEARNING_DELAY_MS);
    learning.confirm_expired();

    let mut fresh_snapshot = None;
    for character in "nihao".chars() {
        let output = session
            .handle(key(sequence, revision, Key::Character(character)))
            .unwrap();
        sequence += 1;
        fresh_snapshot = output.into_iter().find_map(|message| match message {
            EngineMessage::CandidateSnapshot { snapshot, .. } => Some(snapshot),
            _ => None,
        });
        revision = fresh_snapshot.as_ref().unwrap().revision.get();
    }
    assert!(fresh_snapshot.unwrap().candidates.iter().any(|candidate| {
        candidate.text == "旎皓" && candidate.source.starts_with("user_dict")
    }));
}
