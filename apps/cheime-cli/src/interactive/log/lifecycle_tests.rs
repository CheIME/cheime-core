use super::{
    CliInternalEvent, EventSequence, ProtocolEvent, RunId,
    lifecycle::{RunLogLifecycleOperation, open_run_log},
};
use chrono::{DateTime, Utc};
use std::fs;
use tempfile::TempDir;

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

fn event(sequence: u64, reason: impl Into<String>) -> ProtocolEvent {
    ProtocolEvent::internal(
        timestamp(),
        RunId::new("run-lifecycle-83"),
        EventSequence::new(sequence),
        CliInternalEvent::InputIgnored {
            reason: reason.into(),
        },
    )
}

#[test]
fn open_run_log_when_latest_is_absent_creates_exact_timestamped_file_and_keeps_one_content_stream()
{
    // Given
    let directory = TempDir::new().unwrap();
    let first = event(89, "first");
    let second = event(97, "second");

    // When
    let mut run_log = open_run_log(directory.path(), timestamp(), 101).unwrap();
    run_log.writer.append(&first).unwrap();
    run_log.writer.append(&second).unwrap();

    // Then
    let expected_path = directory
        .path()
        .join("cheime-cli-20310203T040506.789Z-101-0.jsonl");
    let latest_path = directory.path().join("latest.jsonl");
    let expected_contents = format!(
        "{}\n{}\n",
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
    assert_eq!(run_log.path, expected_path);
    assert!(run_log.lifecycle_failures.is_empty());
    assert_eq!(
        fs::read_to_string(&run_log.path).unwrap(),
        expected_contents
    );
    assert_eq!(fs::read_to_string(latest_path).unwrap(), expected_contents);
}

#[test]
fn open_run_log_when_latest_exists_replaces_it_with_the_current_run_content() {
    // Given
    let directory = TempDir::new().unwrap();
    let latest_path = directory.path().join("latest.jsonl");
    fs::write(&latest_path, "previous-run\n").unwrap();
    let event = event(103, "replacement");

    // When
    let mut run_log = open_run_log(directory.path(), timestamp(), 107).unwrap();
    run_log.writer.append(&event).unwrap();

    // Then
    let expected_contents = format!("{}\n", serde_json::to_string(&event).unwrap());
    assert!(run_log.lifecycle_failures.is_empty());
    assert_eq!(
        fs::read_to_string(&run_log.path).unwrap(),
        expected_contents
    );
    assert_eq!(fs::read_to_string(latest_path).unwrap(), expected_contents);
}

#[test]
fn open_run_log_when_base_name_collides_allocates_the_next_numeric_suffix_without_reusing_it() {
    // Given
    let directory = TempDir::new().unwrap();
    let collided_path = directory
        .path()
        .join("cheime-cli-20310203T040506.789Z-109-0.jsonl");
    fs::write(&collided_path, "prior-run\n").unwrap();

    // When
    let run_log = open_run_log(directory.path(), timestamp(), 109).unwrap();

    // Then
    assert_eq!(
        run_log.path,
        directory
            .path()
            .join("cheime-cli-20310203T040506.789Z-109-1.jsonl")
    );
    assert_eq!(fs::read_to_string(collided_path).unwrap(), "prior-run\n");
}

#[test]
fn open_run_log_when_latest_cannot_be_replaced_keeps_the_timestamped_writer_usable() {
    // Given
    let directory = TempDir::new().unwrap();
    let latest_path = directory.path().join("latest.jsonl");
    fs::create_dir(&latest_path).unwrap();
    let event = event(113, "link-failure");

    // When
    let mut run_log = open_run_log(directory.path(), timestamp(), 127).unwrap();
    run_log.writer.append(&event).unwrap();

    // Then
    assert!(matches!(
        run_log.lifecycle_failures.as_slice(),
        [remove, link]
            if remove.operation == RunLogLifecycleOperation::RemoveLatest
                && remove.path == latest_path
                && link.operation == RunLogLifecycleOperation::LinkLatest
                && link.path == latest_path
    ));
    assert_eq!(
        fs::read_to_string(run_log.path).unwrap(),
        format!("{}\n", serde_json::to_string(&event).unwrap())
    );
    assert!(latest_path.is_dir());
}
