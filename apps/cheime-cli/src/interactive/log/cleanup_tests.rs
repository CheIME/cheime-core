use super::cleanup::{cleanup_expired_run_logs, cleanup_expired_run_logs_with_remover};
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2031, 2, 3, 4, 5, 6).single().unwrap() + Duration::milliseconds(789)
}

fn run_log_name(timestamp: DateTime<Utc>, process_id: u32, sequence: u32) -> String {
    format!(
        "cheime-cli-{}Z-{process_id}-{sequence}.jsonl",
        timestamp.format("%Y%m%dT%H%M%S%.3f")
    )
}

fn write_log(directory: &Path, name: &str, modified: DateTime<Utc>) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, "event\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified.into()))
        .unwrap();
    path
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[test]
fn cleanup_when_logs_span_cutoff_removes_only_strictly_expired_matching_file() {
    // Given
    let directory = TempDir::new().unwrap();
    let reference = now();
    let expired = write_log(
        directory.path(),
        &run_log_name(
            reference - Duration::days(14) - Duration::milliseconds(1),
            17,
            19,
        ),
        reference,
    );
    let boundary = write_log(
        directory.path(),
        &run_log_name(reference - Duration::days(14), 23, 29),
        reference - Duration::days(30),
    );
    let recent = write_log(
        directory.path(),
        &run_log_name(reference - Duration::days(13), 31, 37),
        reference - Duration::days(30),
    );

    // When
    let report = cleanup_expired_run_logs(directory.path(), reference);

    // Then
    assert_eq!(report.removed, vec![expired.clone()]);
    assert!(!expired.exists());
    assert!(boundary.exists());
    assert!(recent.exists());
}

#[test]
fn cleanup_when_names_are_malformed_or_unrelated_preserves_every_file() {
    // Given
    let directory = TempDir::new().unwrap();
    let reference = now();
    let names = [
        "latest.jsonl",
        "cheime-cli-20310120T040506.78Z-17-19.jsonl",
        "cheime-cli-20310120T040506.789Z-pid-19.jsonl",
        "cheime-cli-20310120T040506.789Z-17-19.jsonl.bak",
        "other-20310120T040506.789Z-17-19.jsonl",
    ];
    let files: Vec<_> = names
        .iter()
        .map(|name| write_log(directory.path(), name, reference - Duration::days(30)))
        .collect();

    // When
    let report = cleanup_expired_run_logs(directory.path(), reference);

    // Then
    assert!(report.removed.is_empty());
    assert!(report.failures.is_empty());
    assert!(files.iter().all(|path| path.exists()));
}

#[test]
fn cleanup_when_matching_entries_are_not_regular_files_preserves_them() {
    // Given
    let directory = TempDir::new().unwrap();
    let reference = now();
    let name = run_log_name(reference - Duration::days(15), 41, 43);
    let matching_directory = directory.path().join(&name);
    fs::create_dir(&matching_directory).unwrap();
    let target = write_log(directory.path(), "target.jsonl", reference);
    let matching_symlink =
        directory
            .path()
            .join(run_log_name(reference - Duration::days(15), 47, 53));
    create_file_symlink(&target, &matching_symlink).unwrap();

    // When
    let report = cleanup_expired_run_logs(directory.path(), reference);

    // Then
    assert!(report.removed.is_empty());
    assert!(report.failures.is_empty());
    assert!(matching_directory.exists());
    assert!(matching_symlink.exists());
}

#[test]
fn cleanup_when_expired_file_cannot_be_removed_reports_failure_and_continues() {
    // Given
    let directory = TempDir::new().unwrap();
    let reference = now();
    let blocked = write_log(
        directory.path(),
        &run_log_name(reference - Duration::days(15), 59, 61),
        reference,
    );
    let removable = write_log(
        directory.path(),
        &run_log_name(reference - Duration::days(15), 67, 71),
        reference,
    );

    // When
    let report = cleanup_expired_run_logs_with_remover(directory.path(), reference, |path| {
        if path == blocked {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            fs::remove_file(path)
        }
    });

    // Then
    assert_eq!(report.removed, vec![removable.clone()]);
    assert!(!removable.exists());
    assert!(blocked.exists());
    assert!(matches!(
        report.failures.as_slice(),
        [failure] if failure.path == blocked && failure.error.kind() == io::ErrorKind::PermissionDenied
    ));
}
