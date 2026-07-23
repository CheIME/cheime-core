use cheime_pipeline::InputPipeline;
use cheime_protocol::FrontendMessage;
use cheime_session::Session;
use cheime_model::{KeyEvent, KeyState, Sequence, CORE_PROTOCOL_VERSION,
    ClientInstanceId, SessionId, SessionEpoch, Revision, DeploymentGeneration};
use cheime_protocol::MessageHeader;
use cheime_user_data::UserStore;
use chrono::Utc;
use parking_lot::Mutex;
use std::io;
use std::path::Path;
use std::sync::Arc;

use super::app::AppState;
use super::input::{self, AppAction, SessionCommand};
use super::log::RunId;
use super::render::frame::build_frame;
use super::render::writer::render_frame;
use super::session::{SessionDispatchError, SessionDriver, SessionApplicationContext};
use super::terminal::Terminal;

pub(crate) fn run_interactive<P: InputPipeline>(
    session: Session<P>,
    store: Arc<Mutex<UserStore>>,
    data_dir: &Path,
) -> io::Result<()> {
    let run_id = RunId::new("interactive");
    let mut driver = SessionDriver::new(session, run_id);
    let mut state = AppState::new();
    let terminal = Terminal::init()?;
    let (cols, rows) = Terminal::size()?;

    // Best-effort run log — open errors are logged to stderr, not fatal.
    let log_path = open_run_log_best_effort(data_dir);

    let result = event_loop(
        &mut driver,
        &mut state,
        Arc::clone(&store),
        &terminal,
        cols,
        rows,
        log_path.as_deref(),
    );

    // Terminal::Drop restores raw mode / alternate screen / cursor.
    drop(terminal);

    result
}

#[allow(clippy::too_many_arguments)]
fn event_loop<P: InputPipeline>(
    driver: &mut SessionDriver<P>,
    state: &mut AppState,
    store: Arc<Mutex<UserStore>>,
    terminal: &Terminal,
    mut cols: u16,
    mut rows: u16,
    log_path: Option<&Path>,
) -> io::Result<()> {
    let mut sequence: u64 = 0;

    loop {
        // ── read key ────────────────────────────────────────────────────
        let ct_key = terminal.read_key()?;

        // Track resize events by polling terminal size after every key
        if let Ok((w, h)) = Terminal::size() {
            cols = w;
            rows = h;
        }

        let input = match input::from_crossterm_key(ct_key) {
            Some(ev) => ev,
            None => continue,
        };

        // ── route → action ──────────────────────────────────────────────
        let action = input::route_key(state, input);

        match action {
            AppAction::Local(local) => {
                state.apply_local(local);
            }
            AppAction::Send(cmd) => {
                let msg = build_frontend_message(cmd, sequence);
                sequence += 1;

                let timestamp = Utc::now();
                let mut user_store = store.lock();
                let ctx = SessionApplicationContext::new(state, &mut user_store);

                match driver.send_and_apply_at(msg, timestamp, ctx) {
                    Ok(_dispatch) => {
                        // Engine messages already applied to state by
                        // send_and_apply_at (snapshot, commits, etc.).
                    }
                    Err(SessionDispatchError::Session { error, .. }) => {
                        state.set_status(format!("engine error: {error}"));
                    }
                    Err(other) => {
                        state.set_status(format!("session error: {other}"));
                    }
                }
            }
            AppAction::Exit => break,
            AppAction::Ignore => {}
        }

        // ── render ──────────────────────────────────────────────────────
        let frame = build_frame(state, cols, rows, log_path);
        let mut stdout = io::stdout();
        render_frame(&mut stdout, &frame)?;
    }

    Ok(())
}

fn build_frontend_message(cmd: SessionCommand, sequence: u64) -> FrontendMessage {
    let header = MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(1),
        epoch: SessionEpoch::new(1),
        sequence: Sequence::new(sequence),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(1),
    };

    match cmd {
        SessionCommand::Key(key) => FrontendMessage::KeyCommand {
            header,
            event: KeyEvent {
                key,
                state: KeyState::default(),
            },
        },
        SessionCommand::Ui(command) => FrontendMessage::UiCommand { header, command },
        SessionCommand::Close => FrontendMessage::CloseSession { header },
    }
}

// ── run log ────────────────────────────────────────────────────────────────

fn open_run_log_best_effort(data_dir: &Path) -> Option<std::path::PathBuf> {
    // Run log lifecycle is wired in a follow-up; for now return None.
    // The log path passed to build_frame just shows "-" on the status bar.
    let _ = data_dir;
    None
}
