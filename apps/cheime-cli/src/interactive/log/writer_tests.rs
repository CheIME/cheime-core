use super::{
    CliInternalEvent, EventSequence, ProtocolEvent, ProtocolEventWriteError, ProtocolEventWriter,
    RunId,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::io::{self, Write};
use tempfile::NamedTempFile;

fn timestamp() -> DateTime<Utc> {
    "2031-02-03T04:05:06.789Z".parse().unwrap()
}

fn event(sequence: u64, reason: impl Into<String>) -> ProtocolEvent {
    ProtocolEvent::internal(
        timestamp(),
        RunId::new("run-writer-83"),
        EventSequence::new(sequence),
        CliInternalEvent::InputIgnored {
            reason: reason.into(),
        },
    )
}

#[test]
fn protocol_event_writer_when_appending_writes_one_compact_flushed_json_line() {
    // Given
    let file = NamedTempFile::new().unwrap();
    let event = event(89, "control_modifier");
    let mut writer = ProtocolEventWriter::new(file.reopen().unwrap());

    // When
    writer.append(&event).unwrap();
    let contents = std::fs::read_to_string(file.path()).unwrap();

    // Then
    assert_eq!(
        contents,
        format!("{}\n", serde_json::to_string(&event).unwrap())
    );
    let lines: Vec<Value> = contents
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<_>>()
        .unwrap();
    assert_eq!(lines, vec![serde_json::to_value(event).unwrap()]);
}

#[test]
fn protocol_event_writer_when_appending_sequential_events_preserves_json_line_order() {
    // Given
    let file = NamedTempFile::new().unwrap();
    let first = event(97, "first");
    let second = event(101, "second");
    let mut writer = ProtocolEventWriter::new(file.reopen().unwrap());

    // When
    writer.append(&first).unwrap();
    writer.append(&second).unwrap();
    let contents = std::fs::read_to_string(file.path()).unwrap();

    // Then
    assert!(contents.ends_with('\n'));
    let lines: Vec<Value> = contents
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<_>>()
        .unwrap();
    assert_eq!(
        lines,
        vec![
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap(),
        ]
    );
}

#[test]
fn protocol_event_writer_when_serialization_hits_a_write_failure_returns_an_error() {
    // Given
    let event = event(103, "x".repeat(16 * 1024));
    let mut writer = ProtocolEventWriter::new(WriteFailure);

    // When
    let result = writer.append(&event);

    // Then
    assert!(matches!(
        result,
        Err(ProtocolEventWriteError::Serialize(error))
            if error.io_error_kind() == Some(io::ErrorKind::BrokenPipe)
    ));
}

#[test]
fn protocol_event_writer_when_flush_fails_returns_an_error_without_panicking() {
    // Given
    let event = event(107, "flush_failure");
    let mut writer = ProtocolEventWriter::new(FlushFailure);

    // When
    let result = writer.append(&event);

    // Then
    assert!(matches!(
        result,
        Err(ProtocolEventWriteError::Io(error)) if error.kind() == io::ErrorKind::Other
    ));
}

struct WriteFailure;

impl Write for WriteFailure {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FlushFailure;

impl Write for FlushFailure {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Other))
    }
}
