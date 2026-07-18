use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    PlatformActionKind, PlatformActionOutcome, PlatformActionResult, Revision, Sequence,
    SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;

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
