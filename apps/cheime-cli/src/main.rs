#![allow(clippy::ptr_arg)]
//! CheIME CLI — 雾凇词库 + 智能学习.
//!
//! Usage:
//!   cargo run -p cheime-cli               # interactive mode (default)
//!   cargo run -p cheime-cli -- --json     # JSON I/O mode (stdin/stdout)

use cheime_dictionary::{parse_body, CompiledIndex, DictColumn};
use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent,
    KeyState, PlatformActionKind, Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::factory::PipelineFactory;
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_session::Session;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_mode = args.iter().any(|a| a == "--json");

    let db_path = data_dir().join("cheime_cli_user.db");
    let user_store = UserStore::open("cli-device", &db_path)
        .unwrap_or_else(|_| UserStore::new("cli-device"));
    let store = Arc::new(Mutex::new(user_store));

    let config: cheime_config::schema::SchemaConfig =
        serde_yaml::from_str("schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n").unwrap();
    let dict_index = load_dict();
    let pipeline = PipelineFactory::build(&config, Some(store.clone()), Some(dict_index), None).unwrap();

    let header = MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION, client: ClientInstanceId::new(1),
        session: SessionId::new(1), epoch: SessionEpoch::new(1),
        sequence: Sequence::new(0), revision: Revision::new(0),
        deployment: DeploymentGeneration::new(1),
    };

    let mut session = Session::new(header, pipeline);

    if json_mode {
        run_json(&mut session, store, &db_path);
    } else {
        run_interactive(&mut session, store, &db_path);
    }
}

// ── Interactive mode ────────────────────────────────────────────────

fn run_interactive(session: &mut Session<impl cheime_pipeline::InputPipeline>, store: Arc<Mutex<UserStore>>, db_path: &PathBuf) {
    println!("CheIME CLI — 雾凇词库 + 智能学习");
    println!("DB: {}\n", db_path.display());
    let mut seq: u64 = 0;

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
            let msg = make_key_msg(seq, key);
            match session.handle(msg) {
                Ok(output) => handle_output_interactive(&output, &store),
                Err(e) => eprintln!("\nError: {e}"),
            }
        }
    }
}

fn handle_output_interactive(output: &[EngineMessage], store: &Arc<Mutex<UserStore>>) {
    for m in output {
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
                if let PlatformActionKind::Commit { text } = &action.kind {
                    println!("\n\x1b[32m→ {}\x1b[0m", text);
                    store.lock().commit_pending(text, "", "quanpin");
                }
            }
            _ => {}
        }
    }
}

// ── JSON I/O mode ───────────────────────────────────────────────────

fn run_json(session: &mut Session<impl cheime_pipeline::InputPipeline>, store: Arc<Mutex<UserStore>>, db_path: &PathBuf) {
    eprintln!("[cheime] JSON mode — {} entries loaded", 539071);
    eprintln!("[cheime] DB: {}", db_path.display());
    let mut seq: u64 = 0;
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        seq += 1;
        let key = match serde_json::from_str::<KeyEvent>(trimmed) {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("[cheime] bad input: {e}");
                continue;
            }
        };
        let msg = FrontendMessage::KeyCommand {
            header: MessageHeader {
                protocol_version: CORE_PROTOCOL_VERSION, client: ClientInstanceId::new(1),
                session: SessionId::new(1), epoch: SessionEpoch::new(1),
                sequence: Sequence::new(seq), revision: Revision::new(0),
                deployment: DeploymentGeneration::new(1),
            },
            event: key,
        };
        match session.handle(msg) {
            Ok(output) => {
                for m in &output {
                    // Commit learns are still applied through the store
                    if let EngineMessage::PlatformAction { action, .. } = m {
                        if let PlatformActionKind::Commit { text } = &action.kind {
                            store.lock().commit_pending(text, "", "quanpin");
                        }
                    }
                    if let Ok(json) = serde_json::to_string(m) {
                        println!("{json}");
                    }
                }
            }
            Err(e) => eprintln!("[cheime] error: {e}"),
        }
    }
}

fn make_key_msg(seq: u64, key: Key) -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: MessageHeader {
            protocol_version: CORE_PROTOCOL_VERSION, client: ClientInstanceId::new(1),
            session: SessionId::new(1), epoch: SessionEpoch::new(1),
            sequence: Sequence::new(seq), revision: Revision::new(0),
            deployment: DeploymentGeneration::new(1),
        },
        event: KeyEvent { key, state: KeyState::default() },
    }
}

// ── Dictionary loader ───────────────────────────────────────────────

fn load_dict() -> Arc<CompiledIndex> {
    let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
    let body = dict_body(raw);
    let columns = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];
    match parse_body(body, columns) {
        Ok(entries) => {
            eprintln!("Loaded {} dict entries (rime_ice base)", entries.len());
            Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1)))
        }
        Err(e) => {
            eprintln!("Dict parse error: {e}");
            Arc::new(CompiledIndex::build(vec![], DeploymentGeneration::new(1)))
        }
    }
}

fn dict_body(raw: &str) -> &str {
    if let Some(pos) = raw.find("\n...\n") {
        &raw[pos + 5..]
    } else {
        raw
    }
}

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CHEIME_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(local).join("cheime")
}
