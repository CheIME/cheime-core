//! CheIME command-line interface for development and testing.
//!
//! Reads keystrokes from stdin, runs them through the full pipeline
//! (session + composable pipeline), and prints preedit + candidates.
//!
//! Usage:
//!   cargo run -p cheime-cli
//!
//! Keys:
//!   a-z        — type pinyin
//!   Backspace  — delete last char
//!   Space/Enter — commit highlighted candidate
//!   1-9        — select candidate by number
//!   - =        — page up / page down
//!   Escape     — cancel composition

use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent,
    KeyState, Revision, Sequence, SessionEpoch, SessionId, UiCommand,
};
use cheime_pipeline::{filter::DedupFilter, processor::DefaultProcessor, ranker::FrequencyRanker,
    segmentor::PinyinSegmentor, translator::PassthroughTranslator, ComposablePipeline};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;
use std::io::{self, BufRead, Write};

fn main() {
    let pipeline = ComposablePipeline::new(
        Box::new(DefaultProcessor::new()),
        Box::new(PinyinSegmentor::new()),
        vec![Box::new(PassthroughTranslator)],
        vec![Box::new(DedupFilter::new())],
        Box::new(FrequencyRanker::new()),
    );

    let header = MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(1),
        epoch: SessionEpoch::new(1),
        sequence: Sequence::new(0),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(1),
    };

    let mut session = Session::new(header, pipeline);
    let mut seq: u64 = 0;

    println!("CheIME CLI — type pinyin, Enter to commit, Esc to quit\n");
    render(&mut io::stdout(), &session);

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap_or_default();
        if line.is_empty() {
            continue;
        }

        for ch in line.chars() {
            seq += 1;
            let (key_event, ui_cmd) = match ch {
                '\x08' | '\x7f' => (Some(KeyEvent { key: Key::Backspace, state: KeyState::default() }), None),
                '\x1b' => break, // Escape → exit
                '\r' | '\n' | ' ' => (Some(KeyEvent { key: Key::Enter, state: KeyState::default() }), None),
                '-' => (None, Some(UiCommand::PreviousPage)),
                '=' => (None, Some(UiCommand::NextPage)),
                d @ '1'..='9' => {
                    let idx = d as usize - '1' as usize;
                    // We need candidate IDs from the current snapshot
                    // For simplicity, just send the index-based selection
                    // In real use, the frontend would know the candidate IDs
                    (None, Some(UiCommand::MoveHighlight(idx as i32)))
                }
                c if c.is_ascii_lowercase() => (
                    Some(KeyEvent { key: Key::Character(c), state: KeyState::default() }),
                    None,
                ),
                _ => continue,
            };

            if let Some(ev) = key_event {
                let msg = make_key_message(seq, &session, ev);
                match session.handle(msg) {
                    Ok(output) => handle_output(&mut io::stdout(), &session, output),
                    Err(e) => eprintln!("Error: {e}"),
                }
            } else if let Some(cmd) = ui_cmd {
                let msg = make_ui_message(seq, &session, cmd);
                match session.handle(msg) {
                    Ok(output) => handle_output(&mut io::stdout(), &session, output),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
        }
    }

    println!("\nGoodbye.");
}

fn make_key_message(
    seq: u64,
    _session: &Session<ComposablePipeline>,
    event: KeyEvent,
) -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: MessageHeader {
            protocol_version: CORE_PROTOCOL_VERSION,
            client: ClientInstanceId::new(1),
            session: SessionId::new(1),
            epoch: SessionEpoch::new(1),
            sequence: Sequence::new(seq),
            revision: Revision::new(0), // Simplified — real code needs current revision
            deployment: DeploymentGeneration::new(1),
        },
        event,
    }
}

fn make_ui_message(
    seq: u64,
    _session: &Session<ComposablePipeline>,
    command: UiCommand,
) -> FrontendMessage {
    FrontendMessage::UiCommand {
        header: MessageHeader {
            protocol_version: CORE_PROTOCOL_VERSION,
            client: ClientInstanceId::new(1),
            session: SessionId::new(1),
            epoch: SessionEpoch::new(1),
            sequence: Sequence::new(seq),
            revision: Revision::new(0),
            deployment: DeploymentGeneration::new(1),
        },
        command,
    }
}

fn handle_output(
    stdout: &mut io::Stdout,
    _session: &Session<ComposablePipeline>,
    messages: Vec<EngineMessage>,
) {
    for msg in messages {
        match msg {
            EngineMessage::CandidateSnapshot { snapshot, .. } => {
                // Clear previous line and render
                print!("\r\x1b[K");
                // Preedit
                if !snapshot.preedit.is_empty() {
                    print!("{} ", snapshot.preedit);
                }
                // Candidates
                for (i, cand) in snapshot.candidates.iter().enumerate() {
                    let marker = if Some(cand.id) == snapshot.highlighted {
                        ">"
                    } else {
                        " "
                    };
                    print!("{}{}.{} ", marker, i + 1, cand.text);
                }
                // Page info
                if !snapshot.candidates.is_empty() {
                    print!("  [{}/{}]", snapshot.page + 1,
                        (snapshot.candidates.len() + snapshot.page_size - 1) / snapshot.page_size.max(1));
                }
                stdout.flush().ok();
            }
            EngineMessage::PlatformAction { action, .. } => {
                use cheime_model::PlatformActionKind;
                match &action.kind {
                    PlatformActionKind::Commit { text } => {
                        println!("\n\x1b[32m→ {}\x1b[0m", text);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn render(stdout: &mut io::Stdout, _session: &Session<ComposablePipeline>) {
    print!("\r\x1b[K> ");
    stdout.flush().ok();
}
