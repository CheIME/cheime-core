use super::*;
use cheime_model::{
    ActionId, Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration, Key, KeyEvent,
    KeyState, PlatformAction, PlatformActionKind, PlatformActionOutcome, PlatformActionResult,
    Revision, SessionEpoch, SessionStatus, UiCommand,
};

fn make_header() -> MessageHeader {
    MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(2),
        epoch: SessionEpoch::new(3),
        sequence: Sequence::new(4),
        revision: Revision::new(5),
        deployment: DeploymentGeneration::new(6),
    }
}

// ── MessageHeader ───────────────────────────────────────────────────────

#[test]
fn message_header_round_trip() {
    let hdr = make_header();
    let json = serde_json::to_string(&hdr).unwrap();
    let back: MessageHeader = serde_json::from_str(&json).unwrap();
    assert_eq!(back, hdr);
}

// ── FrontendMessage ─────────────────────────────────────────────────────

#[test]
fn frontend_open_session_round_trip() {
    let msg = FrontendMessage::OpenSession {
        header: make_header(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: FrontendMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn frontend_close_session_round_trip() {
    let msg = FrontendMessage::CloseSession {
        header: make_header(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: FrontendMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn frontend_key_command_round_trip() {
    let msg = FrontendMessage::KeyCommand {
        header: make_header(),
        event: KeyEvent {
            key: Key::Character('a'),
            state: KeyState::default(),
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: FrontendMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn frontend_ui_command_round_trip() {
    let msg = FrontendMessage::UiCommand {
        header: make_header(),
        command: UiCommand::SelectCandidate {
            epoch: SessionEpoch::new(3),
            snapshot_revision: Revision::new(5),
            candidate_id: CandidateId::new(12),
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: FrontendMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn frontend_platform_action_result_round_trip() {
    let msg = FrontendMessage::PlatformActionResult {
        header: make_header(),
        result: PlatformActionResult {
            action_id: ActionId::new(7),
            outcome: PlatformActionOutcome::Applied,
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: FrontendMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

// ── EngineMessage ───────────────────────────────────────────────────────

#[test]
fn engine_session_opened_round_trip() {
    let msg = EngineMessage::SessionOpened {
        header: make_header(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: EngineMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn engine_session_closed_round_trip() {
    let msg = EngineMessage::SessionClosed {
        header: make_header(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: EngineMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn engine_candidate_snapshot_round_trip() {
    let snap = CandidateSnapshot {
        epoch: SessionEpoch::new(3),
        revision: Revision::new(5),
        deployment: DeploymentGeneration::new(6),
        preedit: "ni".into(),
        cursor: 2,
        candidates: vec![Candidate::text(CandidateId::new(8), "你", "builtin")],
        highlighted: Some(CandidateId::new(8)),
        status: SessionStatus::Composing,
        page_size: 9,
        page: 0,
    };
    let msg = EngineMessage::CandidateSnapshot {
        header: make_header(),
        snapshot: snap,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: EngineMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn engine_platform_action_round_trip() {
    let action = PlatformAction {
        id: ActionId::new(1),
        epoch: SessionEpoch::new(3),
        revision: Revision::new(5),
        kind: PlatformActionKind::Commit { text: "你".into() },
    };
    let msg = EngineMessage::PlatformAction {
        header: make_header(),
        action,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: EngineMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn engine_protocol_rejected_round_trip() {
    let msg = EngineMessage::ProtocolRejected {
        received: 2,
        supported: 1,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: EngineMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, msg);
}
