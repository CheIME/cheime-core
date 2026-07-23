use cheime_protocol::{EngineMessage, FrontendMessage};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Serialize, Serializer};
use std::fmt;
use std::io::{self, BufWriter, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EventDirection {
    FrontendToEngine,
    EngineToFrontend,
    CliInternal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct RunId(String);

impl RunId {
    pub(super) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct EventSequence(u64);

impl EventSequence {
    pub(super) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(super) const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// A typed CLI-local event payload for paths that do not cross the protocol.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) enum CliInternalEvent {
    InputIgnored { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(super) enum ProtocolEventPayload {
    Frontend(FrontendMessage),
    Engine(EngineMessage),
    Internal(CliInternalEvent),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ProtocolEvent {
    #[serde(serialize_with = "serialize_timestamp")]
    pub(super) timestamp: DateTime<Utc>,
    pub(super) run_id: RunId,
    pub(super) sequence: EventSequence,
    pub(super) direction: EventDirection,
    pub(super) event_name: &'static str,
    pub(super) message_type: &'static str,
    pub(super) payload: ProtocolEventPayload,
}

impl ProtocolEvent {
    pub(super) fn frontend(
        timestamp: DateTime<Utc>,
        run_id: RunId,
        sequence: EventSequence,
        message: FrontendMessage,
    ) -> Self {
        let (event_name, message_type) = frontend_identity(&message);
        Self {
            timestamp,
            run_id,
            sequence,
            direction: EventDirection::FrontendToEngine,
            event_name,
            message_type,
            payload: ProtocolEventPayload::Frontend(message),
        }
    }

    pub(super) fn engine(
        timestamp: DateTime<Utc>,
        run_id: RunId,
        sequence: EventSequence,
        message: EngineMessage,
    ) -> Self {
        let (event_name, message_type) = engine_identity(&message);
        Self {
            timestamp,
            run_id,
            sequence,
            direction: EventDirection::EngineToFrontend,
            event_name,
            message_type,
            payload: ProtocolEventPayload::Engine(message),
        }
    }

    pub(super) fn internal(
        timestamp: DateTime<Utc>,
        run_id: RunId,
        sequence: EventSequence,
        message: CliInternalEvent,
    ) -> Self {
        let (event_name, message_type) = internal_identity(&message);
        Self {
            timestamp,
            run_id,
            sequence,
            direction: EventDirection::CliInternal,
            event_name,
            message_type,
            payload: ProtocolEventPayload::Internal(message),
        }
    }
}

/// Appends protocol events to the single timestamped run-log file writer.
pub(super) struct ProtocolEventWriter<W: Write> {
    writer: BufWriter<W>,
}

impl<W> ProtocolEventWriter<W>
where
    W: Write,
{
    pub(super) fn new(writer: W) -> Self {
        Self {
            writer: BufWriter::new(writer),
        }
    }

    /// Serializes, newline-frames, and flushes one event for immediate visibility.
    pub(super) fn append(&mut self, event: &ProtocolEvent) -> Result<(), ProtocolEventWriteError> {
        serde_json::to_writer(&mut self.writer, event)
            .map_err(ProtocolEventWriteError::Serialize)?;
        self.writer
            .write_all(b"\n")
            .map_err(ProtocolEventWriteError::Io)?;
        self.writer.flush().map_err(ProtocolEventWriteError::Io)
    }
}

#[derive(Debug)]
pub(super) enum ProtocolEventWriteError {
    Serialize(serde_json::Error),
    Io(io::Error),
}

impl fmt::Display for ProtocolEventWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialize(error) => write!(formatter, "serializing protocol event: {error}"),
            Self::Io(error) => write!(formatter, "writing protocol event: {error}"),
        }
    }
}

impl std::error::Error for ProtocolEventWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serialize(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

fn frontend_identity(message: &FrontendMessage) -> (&'static str, &'static str) {
    match message {
        FrontendMessage::OpenSession { .. } => ("frontend.open_session", "OpenSession"),
        FrontendMessage::CloseSession { .. } => ("frontend.close_session", "CloseSession"),
        FrontendMessage::KeyCommand { .. } => ("frontend.key_command", "KeyCommand"),
        FrontendMessage::UiCommand { .. } => ("frontend.ui_command", "UiCommand"),
        FrontendMessage::PlatformActionResult { .. } => {
            ("frontend.platform_action_result", "PlatformActionResult")
        }
    }
}

fn engine_identity(message: &EngineMessage) -> (&'static str, &'static str) {
    match message {
        EngineMessage::SessionOpened { .. } => ("engine.session_opened", "SessionOpened"),
        EngineMessage::CandidateSnapshot { .. } => {
            ("engine.candidate_snapshot", "CandidateSnapshot")
        }
        EngineMessage::PlatformAction { .. } => ("engine.platform_action", "PlatformAction"),
        EngineMessage::SessionClosed { .. } => ("engine.session_closed", "SessionClosed"),
        EngineMessage::ProtocolRejected { .. } => ("engine.protocol_rejected", "ProtocolRejected"),
    }
}

fn internal_identity(message: &CliInternalEvent) -> (&'static str, &'static str) {
    match message {
        CliInternalEvent::InputIgnored { .. } => ("cli.input_ignored", "InputIgnored"),
    }
}

fn serialize_timestamp<S>(timestamp: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true))
}

#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod writer_tests;
