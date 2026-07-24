use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, KeyEvent, KeyState, Revision,
    Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::InputPipeline;
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::Session;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::io;
use std::path::Path;
use std::sync::Arc;

use super::app::AppState;
use super::input::{self, AppAction, SessionCommand};
use super::log::RunLog;
use super::render::frame::build_frame;
use super::render::writer::render_frame;
use super::session::SessionDriver;
use super::terminal::Terminal;

pub(crate) fn run_interactive<P: InputPipeline>(
    session: Session<P>,
    store: Arc<Mutex<UserStore>>,
    log_path: &Path,
) -> io::Result<()> {
    let mut log = RunLog::open(log_path)?;
    log.append("session started")?;

    let mut driver = SessionDriver::new(session);
    let mut state = AppState::new();
    let terminal = Terminal::init()?;
    let (columns, rows) = Terminal::size()?;

    let result = event_loop(
        &mut driver,
        &mut state,
        &store,
        &terminal,
        (columns, rows),
        log_path,
        &mut log,
    );
    driver.finish_learning(&store);

    match &result {
        Ok(()) => log.append("session stopped")?,
        Err(error) => {
            let _ = log.append(&format!("terminal error: {error}"));
        }
    }
    result
}

fn event_loop<P: InputPipeline>(
    driver: &mut SessionDriver<P>,
    state: &mut AppState,
    store: &Arc<Mutex<UserStore>>,
    terminal: &Terminal,
    mut size: (u16, u16),
    log_path: &Path,
    log: &mut RunLog,
) -> io::Result<()> {
    let mut sequence = 1_u64;

    loop {
        let event = terminal.read_key()?;
        if let Ok(current_size) = Terminal::size() {
            size = current_size;
        }

        match input::route_key(state, event) {
            AppAction::Local(action) => state.apply_local(action),
            AppAction::Send(command) => {
                let message = build_frontend_message(command, sequence);
                sequence += 1;
                append_best_effort(log, state, &format!("frontend {message:?}"));

                match driver.send_and_apply(message, state, store) {
                    Ok(messages) => {
                        for message in messages {
                            append_best_effort(log, state, &format!("engine {message:?}"));
                        }
                    }
                    Err(error) => {
                        let status = format!("engine error: {error}");
                        append_best_effort(log, state, &status);
                        state.set_status(status);
                    }
                }
            }
            AppAction::Exit => break,
            AppAction::Ignore => continue,
        }

        let frame = build_frame(state, size.0, size.1, log_path);
        render_frame(&mut io::stdout(), &frame)?;
    }

    Ok(())
}

fn append_best_effort(log: &mut RunLog, state: &mut AppState, line: &str) {
    if let Err(error) = log.append(line) {
        state.set_status(format!("log error: {error}"));
    }
}

fn build_frontend_message(command: SessionCommand, sequence: u64) -> FrontendMessage {
    let header = MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(1),
        epoch: SessionEpoch::new(1),
        sequence: Sequence::new(sequence),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(1),
    };

    match command {
        SessionCommand::Key(key) => FrontendMessage::KeyCommand {
            header,
            event: KeyEvent {
                key,
                state: KeyState::default(),
            },
        },
        SessionCommand::Ui(command) => FrontendMessage::UiCommand { header, command },
    }
}
