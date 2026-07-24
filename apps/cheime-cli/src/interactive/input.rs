use super::app::{AppState, LocalAction};
use cheime_model::{Key, UiCommand};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

const NO_HIGHLIGHT_STATUS: &str = "candidate selection requires a highlighted candidate";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AppAction {
    Local(LocalAction),
    Send(SessionCommand),
    Exit,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SessionCommand {
    Key(Key),
    Ui(UiCommand),
}

pub(super) fn route_key(state: &AppState, event: KeyEvent) -> AppAction {
    if event.kind != KeyEventKind::Press || event.modifiers.contains(KeyModifiers::ALT) {
        return AppAction::Ignore;
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        return match event.code {
            KeyCode::Char('c' | 'C') => AppAction::Exit,
            _ => AppAction::Ignore,
        };
    }

    let has_composition = state.has_composition();
    let has_candidates = state
        .snapshot()
        .is_some_and(|snapshot| !snapshot.candidates.is_empty());

    match event.code {
        KeyCode::Char(digit @ '1'..='9') => {
            route_digit(state, digit.to_digit(10).unwrap() as usize - 1, digit)
        }
        KeyCode::Enter => route_enter(state),
        KeyCode::Char(character) if character.is_ascii_alphabetic() => {
            AppAction::Send(SessionCommand::Key(Key::Character(character)))
        }
        KeyCode::Backspace if has_composition => {
            AppAction::Send(SessionCommand::Key(Key::Backspace))
        }
        KeyCode::Backspace => AppAction::Local(LocalAction::Backspace),
        KeyCode::Delete if has_composition => AppAction::Ignore,
        KeyCode::Delete => AppAction::Local(LocalAction::Delete),
        KeyCode::Char(' ') if has_composition => AppAction::Send(SessionCommand::Key(Key::Enter)),
        KeyCode::Char(' ') => AppAction::Local(LocalAction::Insert(' ')),
        KeyCode::Esc if has_composition => AppAction::Send(SessionCommand::Key(Key::Escape)),
        KeyCode::Esc => AppAction::Local(LocalAction::ClearStatus),
        KeyCode::Left if !has_composition => AppAction::Local(LocalAction::MoveLeft),
        KeyCode::Right if !has_composition => AppAction::Local(LocalAction::MoveRight),
        KeyCode::Home if !has_composition => AppAction::Local(LocalAction::MoveHome),
        KeyCode::End if !has_composition => AppAction::Local(LocalAction::MoveEnd),
        KeyCode::Up if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(-1)))
        }
        KeyCode::Down if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(1)))
        }
        KeyCode::PageUp if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::PreviousPage))
        }
        KeyCode::PageDown if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::NextPage))
        }
        _ => AppAction::Ignore,
    }
}

fn route_enter(state: &AppState) -> AppAction {
    match state.snapshot() {
        Some(snapshot) if !snapshot.candidates.is_empty() => match snapshot.highlighted {
            Some(candidate_id) => AppAction::Send(SessionCommand::Ui(UiCommand::SelectCandidate {
                epoch: snapshot.epoch,
                snapshot_revision: snapshot.revision,
                candidate_id,
            })),
            None => AppAction::Local(LocalAction::SetStatus(NO_HIGHLIGHT_STATUS)),
        },
        Some(snapshot) if !snapshot.preedit.is_empty() => {
            AppAction::Send(SessionCommand::Key(Key::Enter))
        }
        Some(_) | None => AppAction::Ignore,
    }
}

fn route_digit(state: &AppState, index: usize, digit: char) -> AppAction {
    match state.snapshot().and_then(|snapshot| {
        snapshot
            .candidates
            .get(index)
            .map(|candidate| (snapshot, candidate))
    }) {
        Some((snapshot, candidate)) => {
            AppAction::Send(SessionCommand::Ui(UiCommand::SelectCandidate {
                epoch: snapshot.epoch,
                snapshot_revision: snapshot.revision,
                candidate_id: candidate.id,
            }))
        }
        None if state.has_composition() => {
            AppAction::Send(SessionCommand::Key(Key::Character(digit)))
        }
        None => AppAction::Local(LocalAction::Insert(digit)),
    }
}

#[cfg(test)]
mod input_tests;
