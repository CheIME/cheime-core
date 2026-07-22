use super::*;

// ── ID types ───────────────────────────────────────────────────────────

#[test]
fn candidate_id_round_trip() {
    let id = CandidateId::new(42);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "42");
    let back: CandidateId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn session_id_round_trip() {
    let id = SessionId::new(7);
    let json = serde_json::to_string(&id).unwrap();
    let back: SessionId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn session_epoch_round_trip() {
    let id = SessionEpoch::new(3);
    let json = serde_json::to_string(&id).unwrap();
    let back: SessionEpoch = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn client_instance_id_round_trip() {
    let id = ClientInstanceId::new(1);
    let json = serde_json::to_string(&id).unwrap();
    let back: ClientInstanceId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn action_id_round_trip() {
    let id = ActionId::new(99);
    let json = serde_json::to_string(&id).unwrap();
    let back: ActionId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn deployment_generation_round_trip() {
    let id = DeploymentGeneration::new(5);
    let json = serde_json::to_string(&id).unwrap();
    let back: DeploymentGeneration = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn sequence_round_trip() {
    let id = Sequence::new(10);
    let json = serde_json::to_string(&id).unwrap();
    let back: Sequence = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn revision_round_trip() {
    let r = Revision::new(41);
    let json = serde_json::to_string(&r).unwrap();
    let back: Revision = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}

// ── Candidate types ─────────────────────────────────────────────────────

#[test]
fn candidate_round_trip() {
    let c = Candidate {
        id: CandidateId::new(8),
        text: "你好".into(),
        annotation: Some("nǐ hǎo".into()),
        source: "builtin".into(),
        is_emoji: false,
    };
    let json = serde_json::to_string(&c).unwrap();
    let back: Candidate = serde_json::from_str(&json).unwrap();
    assert_eq!(back, c);
}

#[test]
fn candidate_round_trip_no_annotation() {
    let c = Candidate::text(CandidateId::new(1), "x", "test");
    let json = serde_json::to_string(&c).unwrap();
    let back: Candidate = serde_json::from_str(&json).unwrap();
    assert_eq!(back, c);
}

#[test]
fn candidate_deserialize_minimal_json() {
    let json = r#"{"id":1,"text":"a","source":"t"}"#;
    let c: Candidate = serde_json::from_str(json).unwrap();
    assert!(!c.is_emoji);
    assert!(c.annotation.is_none());
}

#[test]
fn candidate_snapshot_round_trip() {
    let snap = CandidateSnapshot {
        epoch: SessionEpoch::new(2),
        revision: Revision::new(3),
        deployment: DeploymentGeneration::new(5),
        preedit: "ni".into(),
        cursor: 2,
        candidates: vec![
            Candidate::text(CandidateId::new(8), "你", "builtin"),
            Candidate::text(CandidateId::new(9), "尼", "builtin"),
        ],
        highlighted: Some(CandidateId::new(8)),
        status: SessionStatus::Composing,
        page_size: 9,
        page: 0,
    };
    let json = serde_json::to_string(&snap).unwrap();
    let back: CandidateSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
}

#[test]
fn session_status_all_variants_round_trip() {
    for status in [
        SessionStatus::Ready,
        SessionStatus::Composing,
        SessionStatus::CommitPending,
        SessionStatus::Transparent,
    ] {
        let json = serde_json::to_string(&status).unwrap();
        let back: SessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }
}

// ── Input types ─────────────────────────────────────────────────────────

#[test]
fn key_all_variants_round_trip() {
    let cases = [
        Key::Character('a'),
        Key::Backspace,
        Key::Escape,
        Key::Enter,
        Key::Space,
    ];
    for key in &cases {
        let json = serde_json::to_string(key).unwrap();
        let back: Key = serde_json::from_str(&json).unwrap();
        assert_eq!(back, *key);
    }
}

#[test]
fn key_character_deserialize_from_known_json() {
    let json = r#"{"Character":"z"}"#;
    let k: Key = serde_json::from_str(json).unwrap();
    assert_eq!(k, Key::Character('z'));
}

#[test]
fn key_state_round_trip() {
    let state = KeyState {
        shift: true,
        control: false,
        alt: true,
    };
    let json = serde_json::to_string(&state).unwrap();
    let back: KeyState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, state);
}

#[test]
fn key_state_default_is_all_false() {
    let state = KeyState::default();
    assert!(!state.shift);
    assert!(!state.control);
    assert!(!state.alt);
}

#[test]
fn key_event_round_trip() {
    let event = KeyEvent {
        key: Key::Character('n'),
        state: KeyState {
            shift: false,
            control: true,
            alt: false,
        },
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: KeyEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back, event);
}

// ── UiCommand ───────────────────────────────────────────────────────────

#[test]
fn ui_command_select_candidate_round_trip() {
    let cmd = UiCommand::SelectCandidate {
        epoch: SessionEpoch::new(4),
        snapshot_revision: Revision::new(9),
        candidate_id: CandidateId::new(12),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    let back: UiCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(back, cmd);
}

#[test]
fn ui_command_move_highlight_round_trip() {
    let cmd = UiCommand::MoveHighlight(-1);
    let json = serde_json::to_string(&cmd).unwrap();
    let back: UiCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(back, UiCommand::MoveHighlight(-1));
}

#[test]
fn ui_command_next_page_round_trip() {
    let cmd = UiCommand::NextPage;
    let json = serde_json::to_string(&cmd).unwrap();
    let back: UiCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(back, UiCommand::NextPage);
}

#[test]
fn ui_command_previous_page_round_trip() {
    let cmd = UiCommand::PreviousPage;
    let json = serde_json::to_string(&cmd).unwrap();
    let back: UiCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(back, UiCommand::PreviousPage);
}

#[test]
fn ui_command_dismiss_round_trip() {
    let cmd = UiCommand::Dismiss;
    let json = serde_json::to_string(&cmd).unwrap();
    let back: UiCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(back, UiCommand::Dismiss);
}

// ── PlatformAction ──────────────────────────────────────────────────────

#[test]
fn platform_action_kind_set_preedit_round_trip() {
    let kind = PlatformActionKind::SetPreedit {
        text: "ni".into(),
        cursor: 2,
    };
    let json = serde_json::to_string(&kind).unwrap();
    let back: PlatformActionKind = serde_json::from_str(&json).unwrap();
    assert_eq!(back, kind);
}

#[test]
fn platform_action_kind_commit_round_trip() {
    let kind = PlatformActionKind::Commit {
        text: "你好".into(),
    };
    let json = serde_json::to_string(&kind).unwrap();
    let back: PlatformActionKind = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back,
        PlatformActionKind::Commit {
            text: "你好".into()
        }
    );
}

#[test]
fn platform_action_kind_cancel_composition_round_trip() {
    let kind = PlatformActionKind::CancelComposition;
    let json = serde_json::to_string(&kind).unwrap();
    let back: PlatformActionKind = serde_json::from_str(&json).unwrap();
    assert_eq!(back, PlatformActionKind::CancelComposition);
}

#[test]
fn platform_action_round_trip() {
    let action = PlatformAction {
        id: ActionId::new(1),
        epoch: SessionEpoch::new(2),
        revision: Revision::new(3),
        kind: PlatformActionKind::Commit { text: "你".into() },
    };
    let json = serde_json::to_string(&action).unwrap();
    let back: PlatformAction = serde_json::from_str(&json).unwrap();
    assert_eq!(back, action);
}

// ── PlatformActionOutcome / PlatformActionResult ────────────────────────

#[test]
fn platform_action_outcome_applied_round_trip() {
    let outcome = PlatformActionOutcome::Applied;
    let json = serde_json::to_string(&outcome).unwrap();
    let back: PlatformActionOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(back, PlatformActionOutcome::Applied);
}

#[test]
fn platform_action_outcome_rejected_round_trip() {
    let outcome = PlatformActionOutcome::Rejected {
        reason: "stale epoch".into(),
    };
    let json = serde_json::to_string(&outcome).unwrap();
    let back: PlatformActionOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back,
        PlatformActionOutcome::Rejected {
            reason: "stale epoch".into()
        }
    );
}

#[test]
fn platform_action_result_round_trip() {
    let result = PlatformActionResult {
        action_id: ActionId::new(7),
        outcome: PlatformActionOutcome::Applied,
    };
    let json = serde_json::to_string(&result).unwrap();
    let back: PlatformActionResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back, result);
}
