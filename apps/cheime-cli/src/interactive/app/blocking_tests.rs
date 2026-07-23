use super::*;
use cheime_model::{
    CandidateSnapshot, DeploymentGeneration, Revision, SessionEpoch, SessionStatus,
};

fn composition_snapshot(preedit: &str, status: SessionStatus) -> CandidateSnapshot {
    CandidateSnapshot {
        epoch: SessionEpoch::new(1),
        revision: Revision::new(1),
        deployment: DeploymentGeneration::new(1),
        preedit: preedit.into(),
        cursor: preedit.len(),
        candidates: vec![],
        highlighted: None,
        status,
        page_size: 9,
        page: 0,
    }
}

fn document_with_interior_cursor() -> AppState {
    let mut state = AppState::new();
    state.document.insert("a你b");
    state.document.move_left();
    state.document.move_left();
    state
}

#[test]
fn document_actions_preserve_text_and_cursor_when_composition_is_active() {
    let compositions = [
        ("nonempty preedit", "ni", SessionStatus::Composing),
        ("commit pending", "", SessionStatus::CommitPending),
    ];
    let actions = [
        ("insert", LocalAction::Insert('x')),
        ("backspace", LocalAction::Backspace),
        ("delete", LocalAction::Delete),
        ("move left", LocalAction::MoveLeft),
        ("move right", LocalAction::MoveRight),
        ("move home", LocalAction::MoveHome),
        ("move end", LocalAction::MoveEnd),
    ];

    for (composition_name, preedit, status) in compositions {
        for (action_name, action) in actions {
            let mut state = document_with_interior_cursor();
            state.set_snapshot(composition_snapshot(preedit, status.clone()));
            let before = (
                state.document().text().to_owned(),
                state.document().cursor(),
            );

            state.apply_local(action);

            let after = (state.document().text(), state.document().cursor());
            assert_eq!(
                after,
                (before.0.as_str(), before.1),
                "{composition_name}: {action_name}"
            );
        }
    }
}
