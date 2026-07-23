use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RUN_LOG_PREFIX: &str = "cheime-cli-";
const RUN_LOG_TIMESTAMP_FORMAT: &str = "%Y%m%dT%H%M%S%.3fZ";
const RUN_LOG_RETENTION: Duration = Duration::days(14);

#[derive(Debug, Default)]
pub(in crate::interactive) struct RunLogCleanupReport {
    pub(in crate::interactive) removed: Vec<PathBuf>,
    pub(in crate::interactive) failures: Vec<RunLogCleanupFailure>,
}

#[derive(Debug)]
pub(in crate::interactive) struct RunLogCleanupFailure {
    pub(in crate::interactive) path: PathBuf,
    pub(in crate::interactive) error: io::Error,
}

/// Removes only timestamped CLI run logs older than the retention cutoff.
pub(in crate::interactive) fn cleanup_expired_run_logs(
    directory: &Path,
    now: DateTime<Utc>,
) -> RunLogCleanupReport {
    cleanup_expired_run_logs_inner(directory, now, |path| fs::remove_file(path))
}

#[cfg(test)]
pub(in crate::interactive) fn cleanup_expired_run_logs_with_remover<Remove>(
    directory: &Path,
    now: DateTime<Utc>,
    remover: Remove,
) -> RunLogCleanupReport
where
    Remove: FnMut(&Path) -> io::Result<()>,
{
    cleanup_expired_run_logs_inner(directory, now, remover)
}

fn cleanup_expired_run_logs_inner<Remove>(
    directory: &Path,
    now: DateTime<Utc>,
    mut remover: Remove,
) -> RunLogCleanupReport
where
    Remove: FnMut(&Path) -> io::Result<()>,
{
    let Some(cutoff) = now.checked_sub_signed(RUN_LOG_RETENTION) else {
        return RunLogCleanupReport::default();
    };
    let mut report = RunLogCleanupReport::default();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            report.record_failure(directory, error);
            return report;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.record_failure(directory, error);
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                report.record_failure(&path, error);
                continue;
            }
        };
        if !file_type.is_file() {
            continue;
        }
        let Some(timestamp) = run_log_timestamp(&entry.file_name()) else {
            continue;
        };
        if timestamp >= cutoff {
            continue;
        }
        match remover(&path) {
            Ok(()) => report.removed.push(path),
            Err(error) => report.record_failure(&path, error),
        }
    }

    report
}

impl RunLogCleanupReport {
    fn record_failure(&mut self, path: &Path, error: io::Error) {
        self.failures.push(RunLogCleanupFailure {
            path: path.to_path_buf(),
            error,
        });
    }
}

fn run_log_timestamp(file_name: &OsStr) -> Option<DateTime<Utc>> {
    let file_name = file_name.to_str()?;
    let remainder = file_name.strip_prefix(RUN_LOG_PREFIX)?;
    let (timestamp, process_and_sequence) = remainder.split_once('-')?;
    let (process_id, sequence_and_extension) = process_and_sequence.split_once('-')?;
    let sequence = sequence_and_extension.strip_suffix(".jsonl")?;
    if !timestamp_has_exact_shape(timestamp)
        || !is_nonempty_ascii_digits(process_id)
        || !is_nonempty_ascii_digits(sequence)
    {
        return None;
    }
    let timestamp = NaiveDateTime::parse_from_str(timestamp, RUN_LOG_TIMESTAMP_FORMAT).ok()?;
    Some(Utc.from_utc_datetime(&timestamp))
}

fn timestamp_has_exact_shape(timestamp: &str) -> bool {
    let bytes = timestamp.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    bytes[8] == b'T'
        && bytes[15] == b'.'
        && bytes[19] == b'Z'
        && bytes[..8]
            .iter()
            .chain(bytes[9..15].iter())
            .chain(bytes[16..19].iter())
            .all(u8::is_ascii_digit)
}

fn is_nonempty_ascii_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}
