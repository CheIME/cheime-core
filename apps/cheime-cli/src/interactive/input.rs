use super::app::{AppState, LocalAction};
use cheime_model::{Key, UiCommand};

const NO_HIGHLIGHT_STATUS: &str = "candidate selection requires a highlighted candidate";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InputKind {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct InputModifiers {
    pub(super) shift: bool,
    pub(super) control: bool,
    pub(super) alt: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InputKey {
    Character(char),
    Backspace,
    Delete,
    Enter,
    Space,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct InputEvent {
    pub(super) key: InputKey,
    pub(super) modifiers: InputModifiers,
    pub(super) kind: InputKind,
}

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
    Close,
}

pub(super) fn route_key(state: &AppState, event: InputEvent) -> AppAction {
    match event.kind {
        InputKind::Press => route_press(state, event),
        InputKind::Repeat | InputKind::Release => AppAction::Ignore,
    }
}

fn route_press(state: &AppState, event: InputEvent) -> AppAction {
    if event.modifiers.alt {
        return AppAction::Ignore;
    }

    if event.modifiers.control {
        return match event.key {
            InputKey::Character('c' | 'C') => AppAction::Exit,
            InputKey::Up => AppAction::Local(LocalAction::ScrollUp),
            InputKey::Down => AppAction::Local(LocalAction::ScrollDown),
            InputKey::Character(_)
            | InputKey::Backspace
            | InputKey::Delete
            | InputKey::Enter
            | InputKey::Space
            | InputKey::Escape
            | InputKey::Left
            | InputKey::Right
            | InputKey::Home
            | InputKey::End
            | InputKey::PageUp
            | InputKey::PageDown
            | InputKey::F2 => AppAction::Ignore,
        };
    }

    let has_composition = state.has_composition();
    let has_candidates = state
        .snapshot()
        .is_some_and(|snapshot| !snapshot.candidates.is_empty());

    match event.key {
        InputKey::Character('1') => route_digit(state, 0, '1'),
        InputKey::Character('2') => route_digit(state, 1, '2'),
        InputKey::Character('3') => route_digit(state, 2, '3'),
        InputKey::Character('4') => route_digit(state, 3, '4'),
        InputKey::Character('5') => route_digit(state, 4, '5'),
        InputKey::Character('6') => route_digit(state, 5, '6'),
        InputKey::Character('7') => route_digit(state, 6, '7'),
        InputKey::Character('8') => route_digit(state, 7, '8'),
        InputKey::Character('9') => route_digit(state, 8, '9'),
        InputKey::Enter => match state.snapshot() {
            Some(snapshot) if !snapshot.candidates.is_empty() => match snapshot.highlighted {
                Some(candidate_id) => {
                    AppAction::Send(SessionCommand::Ui(UiCommand::SelectCandidate {
                        epoch: snapshot.epoch,
                        snapshot_revision: snapshot.revision,
                        candidate_id,
                    }))
                }
                None => AppAction::Local(LocalAction::SetStatus(NO_HIGHLIGHT_STATUS)),
            },
            Some(snapshot) if !snapshot.preedit.is_empty() => {
                AppAction::Send(SessionCommand::Key(Key::Enter))
            }
            Some(_) | None => AppAction::Ignore,
        },
        InputKey::Character(ch) if ch.is_ascii_alphabetic() => {
            AppAction::Send(SessionCommand::Key(Key::Character(ch)))
        }
        InputKey::Character(_) => AppAction::Ignore,
        InputKey::Backspace if has_composition => {
            AppAction::Send(SessionCommand::Key(Key::Backspace))
        }
        InputKey::Backspace => AppAction::Local(LocalAction::Backspace),
        InputKey::Delete if has_composition => AppAction::Ignore,
        InputKey::Delete => AppAction::Local(LocalAction::Delete),
        InputKey::Space if has_composition => AppAction::Send(SessionCommand::Key(Key::Enter)),
        InputKey::Space => AppAction::Local(LocalAction::Insert(' ')),
        InputKey::Escape if has_composition => AppAction::Send(SessionCommand::Key(Key::Escape)),
        InputKey::Escape => AppAction::Local(LocalAction::ClearStatus),
        InputKey::Left | InputKey::Right | InputKey::Home | InputKey::End if has_composition => {
            AppAction::Ignore
        }
        InputKey::Left => AppAction::Local(LocalAction::MoveLeft),
        InputKey::Right => AppAction::Local(LocalAction::MoveRight),
        InputKey::Home => AppAction::Local(LocalAction::MoveHome),
        InputKey::End => AppAction::Local(LocalAction::MoveEnd),
        InputKey::Up if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(-1)))
        }
        InputKey::Up => AppAction::Ignore,
        InputKey::Down if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::MoveHighlight(1)))
        }
        InputKey::Down => AppAction::Ignore,
        InputKey::PageUp if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::PreviousPage))
        }
        InputKey::PageUp => AppAction::Ignore,
        InputKey::PageDown if has_candidates => {
            AppAction::Send(SessionCommand::Ui(UiCommand::NextPage))
        }
        InputKey::PageDown => AppAction::Ignore,
        InputKey::F2 => AppAction::Local(LocalAction::ToggleDetailMode),
    }
}

fn route_digit(state: &AppState, index: usize, digit: char) -> AppAction {
    match state.snapshot() {
        Some(snapshot) => match snapshot.candidates.get(index) {
            Some(candidate) => AppAction::Send(SessionCommand::Ui(UiCommand::SelectCandidate {
                epoch: snapshot.epoch,
                snapshot_revision: snapshot.revision,
                candidate_id: candidate.id,
            })),
            None => match state.has_composition() {
                true => AppAction::Send(SessionCommand::Key(Key::Character(digit))),
                false => AppAction::Local(LocalAction::Insert(digit)),
            },
        },
        None => AppAction::Local(LocalAction::Insert(digit)),
    }
}

#[cfg(test)]
mod candidate_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod modifier_tests;
