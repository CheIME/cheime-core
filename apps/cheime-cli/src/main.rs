//! CheIME CLI — 雾凇词库 + 智能学习.
//!
//! Usage:
//!   cargo run -p cheime-cli -- --dict path/to/dictionaries

use cheime_dictionary::{CompiledIndex, DictColumn, parse_body};
use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Revision, Sequence,
    SessionEpoch, SessionId,
};
use cheime_pipeline::factory::PipelineFactory;
use cheime_protocol::MessageHeader;
use cheime_session::Session;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

mod interactive;

#[cfg(test)]
mod main_tests;

#[derive(Debug, Eq, PartialEq)]
struct CliOptions {
    dictionary_dir: PathBuf,
    log: PathBuf,
}

impl CliOptions {
    fn parse_from(
        arguments: impl IntoIterator<Item = OsString>,
        data_dir: &Path,
    ) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let mut dictionary_dir = None;
        let mut log = None;

        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--dict") => {
                    dictionary_dir =
                        Some(PathBuf::from(arguments.next().ok_or_else(|| {
                            String::from("--dict requires a directory path")
                        })?));
                }
                Some("--log") => {
                    log = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| String::from("--log requires a file path"))?,
                    ));
                }
                Some(other) => return Err(format!("unknown argument: {other}")),
                None => return Err(String::from("arguments must be valid Unicode")),
            }
        }

        Ok(Self {
            dictionary_dir: dictionary_dir
                .ok_or_else(|| String::from("missing required --dict <DIR> argument"))?,
            log: log.unwrap_or_else(|| data_dir.join("logs").join("cheime-cli.log")),
        })
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cheime: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let data_dir = data_dir();
    let options = CliOptions::parse_from(std::env::args_os().skip(1), &data_dir)?;
    let dict_index = load_dict_directory(&options.dictionary_dir)?;

    let user_store = open_user_store(&data_dir)?;
    let store = Arc::new(Mutex::new(user_store));

    let config: cheime_config::schema::SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n",
    )
    .unwrap();
    eprintln!(
        "Loaded {} entries from {}",
        dict_index.total_entries(),
        options.dictionary_dir.display()
    );
    let pipeline =
        PipelineFactory::build(&config, Some(store.clone()), Some(dict_index), None).unwrap();

    let header = MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(1),
        epoch: SessionEpoch::new(1),
        sequence: Sequence::new(0),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(1),
    };

    let session = Session::new(header, pipeline);
    run_interactive(session, &options.log)?;
    Ok(())
}

fn open_user_store(data_dir: &Path) -> Result<UserStore, String> {
    fs::create_dir_all(data_dir)
        .map_err(|error| format!("create data directory {}: {error}", data_dir.display()))?;
    let db_path = data_dir.join("cheime_cli_user.db");
    UserStore::open("cli-device", &db_path)
        .map_err(|error| format!("open user database {}: {error}", db_path.display()))
}

// ── Interactive mode ────────────────────────────────────────────────────────

fn run_interactive(
    session: Session<impl cheime_pipeline::InputPipeline>,
    log_path: &Path,
) -> Result<(), String> {
    interactive::tui::run_interactive(session, log_path)
        .map_err(|error| format!("terminal error: {error}"))
}

// ── Dictionary loader ───────────────────────────────────────────────

fn load_dict_directory(directory: &Path) -> Result<Arc<CompiledIndex>, String> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("read dictionary directory {}: {error}", directory.display()))?
        .map(|entry| {
            entry.map(|entry| entry.path()).map_err(|error| {
                format!("read dictionary directory {}: {error}", directory.display())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".dict.yaml"))
    });
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "no .dict.yaml files found in {}",
            directory.display()
        ));
    }

    let columns = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];
    let mut entries = Vec::new();
    for path in paths {
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let parsed = parse_body(dict_body(&raw), columns)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        if parsed.is_empty() {
            return Err(format!("dictionary is empty: {}", path.display()));
        }
        entries.extend(parsed);
    }

    Ok(Arc::new(CompiledIndex::build(
        entries,
        DeploymentGeneration::new(1),
    )))
}

fn dict_body(raw: &str) -> &str {
    for marker in ["\r\n...\r\n", "\n...\n"] {
        if let Some(position) = raw.find(marker) {
            return &raw[position + marker.len()..];
        }
    }
    raw
}

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CHEIME_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(local).join("cheime")
}
