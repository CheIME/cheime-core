use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::Session;
use cheime_user_data::UserStore;
use chrono::{DateTime, Utc};

use super::{
    app::AppState,
    log::RunId,
    session::{SessionApplicationContext, SessionDriver},
};

fn header() -> MessageHeader {
    MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(10),
        session: SessionId::new(20),
        epoch: SessionEpoch::new(30),
        sequence: Sequence::new(0),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(40),
    }
}

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

#[test]
fn interactive_composition_root_can_construct_context_and_drain() {
    // Given
    let session = Session::new(header(), BuiltinPipeline::new([]));
    let mut driver = SessionDriver::new(session, RunId::new("run-visibility"));
    let mut state = AppState::new();
    let mut store = UserStore::new("visibility");
    let message = FrontendMessage::KeyCommand {
        header: MessageHeader {
            sequence: Sequence::new(1),
            ..header()
        },
        event: KeyEvent {
            key: Key::Character('n'),
            state: KeyState::default(),
        },
    };

    // When
    let dispatch = driver
        .send_and_apply_at(
            message,
            timestamp(),
            SessionApplicationContext::new(&mut state, &mut store),
        )
        .unwrap();

    // Then
    assert_eq!(state.document().text(), "");
    drop(dispatch);
}
