//! CheIME CLI — demonstrates learned words across sessions.
//!
//! Usage: cargo run -p cheime-cli

use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent,
    KeyState, Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::factory::PipelineFactory;
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;
use cheime_user_data::{UserEvent, UserStore};
use parking_lot::Mutex;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let db_path = dirs().join("cheime_cli_user.db");
    let user_store = UserStore::open("cli-device", &db_path)
        .unwrap_or_else(|_| {
            eprintln!("warning: could not open user db, using in-memory store");
            UserStore::new("cli-device")
        });
    let store = Arc::new(Mutex::new(user_store));

    let config = serde_yaml::from_str("schema_version: 1\nengine: {}\n").unwrap();
    let pipeline = PipelineFactory::build(&config, Some(store.clone())).unwrap();

    let header = MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1), session: SessionId::new(1),
        epoch: SessionEpoch::new(1), sequence: Sequence::new(0),
        revision: Revision::new(0), deployment: DeploymentGeneration::new(1),
    };

    let mut session = Session::new(header, pipeline);
    let mut seq: u64 = 0;

    println!("CheIME CLI — type pinyin, Enter to commit, Ctrl+C to quit");
    println!("DB: {}\n", db_path.display());
    render(&mut io::stdout(), &session);

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.is_empty() { continue; }

        for ch in line.chars() {
            if ch == '\x1b' { return; }
            seq += 1;

            let key = match ch {
                '\x08' | '\x7f' => Key::Backspace,
                '\r' | '\n' | ' ' => Key::Enter,
                c if c.is_ascii_lowercase() => Key::Character(c),
                _ => continue,
            };

            let msg = FrontendMessage::KeyCommand {
                header: MessageHeader {
                    protocol_version: CORE_PROTOCOL_VERSION,
                    client: ClientInstanceId::new(1), session: SessionId::new(1),
                    epoch: SessionEpoch::new(1), sequence: Sequence::new(seq),
                    revision: Revision::new(0), deployment: DeploymentGeneration::new(1),
                },
                event: KeyEvent { key, state: KeyState::default() },
            };

            match session.handle(msg) {
                Ok(output) => {
                    for m in &output {
                        match m {
                            EngineMessage::CandidateSnapshot { snapshot, .. } => {
                                print!("\r\x1b[K");
                                if !snapshot.preedit.is_empty() { print!("{} ", snapshot.preedit); }
                                for (i, c) in snapshot.candidates.iter().enumerate() {
                                    let mark = if Some(c.id) == snapshot.highlighted { ">" } else { " " };
                                    print!("{}{}.{} ", mark, i + 1, c.text);
                                }
                                io::stdout().flush().ok();
                            }
                            EngineMessage::PlatformAction { action, .. } => {
                                use cheime_model::PlatformActionKind;
                                if let PlatformActionKind::Commit { text } = &action.kind {
                                    println!("\n\x1b[32m→ {}\x1b[0m", text);
                                    // Learn the word
                                    store.lock().apply(UserEvent::learn_word(
                                        "cli-device", "quanpin", text, "",
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => eprintln!("\nError: {e}"),
            }
        }
    }
}

fn dirs() -> PathBuf {
    std::env::var("CHEIME_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = dirs_fallback();
            p.push("cheime");
            std::fs::create_dir_all(&p).ok();
            p
        })
}

#[cfg(target_os = "windows")]
fn dirs_fallback() -> PathBuf {
    std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}
#[cfg(not(target_os = "windows"))]
fn dirs_fallback() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

fn render(stdout: &mut io::Stdout, _session: &Session<cheime_pipeline::ComposablePipeline>) {
    print!("\r\x1b[K> ");
    stdout.flush().ok();
}
