#![allow(dead_code)] // Scaffolded lifecycle infrastructure; will be wired in later tasks

use super::ProtocolEventWriter;
use chrono::{DateTime, Utc};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

const LATEST_RUN_LOG_NAME: &str = "latest.jsonl";

pub(in crate::interactive) struct RunLog {
    pub(in crate::interactive) path: PathBuf,
    pub(in crate::interactive) writer: ProtocolEventWriter<File>,
    pub(in crate::interactive) lifecycle_failures: Vec<RunLogLifecycleFailure>,
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::interactive) enum RunLogLifecycleOperation {
    RemoveLatest,
    LinkLatest,
}

#[derive(Debug)]
pub(in crate::interactive) struct RunLogLifecycleFailure {
    pub(in crate::interactive) operation: RunLogLifecycleOperation,
    pub(in crate::interactive) path: PathBuf,
    pub(in crate::interactive) error: io::Error,
}

/// Creates the sole timestamped writer and refreshes the best-effort latest link.
pub(in crate::interactive) fn open_run_log(
    directory: &Path,
    timestamp: DateTime<Utc>,
    process_id: u32,
) -> io::Result<RunLog> {
    let (path, file) = create_run_file(directory, timestamp, process_id)?;
    let lifecycle_failures = refresh_latest_link(&path);

    Ok(RunLog {
        path,
        writer: ProtocolEventWriter::new(file),
        lifecycle_failures,
    })
}

fn create_run_file(
    directory: &Path,
    timestamp: DateTime<Utc>,
    process_id: u32,
) -> io::Result<(PathBuf, File)> {
    let timestamp = timestamp.format("%Y%m%dT%H%M%S%.3fZ");
    let mut suffix = 0_u64;

    loop {
        let path = directory.join(format!(
            "cheime-cli-{timestamp}-{process_id}-{suffix}.jsonl"
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                suffix = suffix.checked_add(1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::AlreadyExists, "run log suffix exhausted")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn refresh_latest_link(path: &Path) -> Vec<RunLogLifecycleFailure> {
    let latest_path = path.with_file_name(LATEST_RUN_LOG_NAME);
    let mut failures = Vec::new();

    if let Err(error) = fs::remove_file(&latest_path) {
        if error.kind() != io::ErrorKind::NotFound {
            failures.push(RunLogLifecycleFailure {
                operation: RunLogLifecycleOperation::RemoveLatest,
                path: latest_path.clone(),
                error,
            });
        }
    }
    if let Err(error) = fs::hard_link(path, &latest_path) {
        failures.push(RunLogLifecycleFailure {
            operation: RunLogLifecycleOperation::LinkLatest,
            path: latest_path,
            error,
        });
    }

    failures
}
