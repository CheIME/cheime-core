use cheime_pipeline::InputPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage};
use cheime_session::{Session, SessionError};
use chrono::{DateTime, Utc};
use std::fmt;

use super::log::{EventSequence, ProtocolEvent, RunId};

mod drain;

pub(super) struct SessionDriver<P> {
    session: Session<P>,
    run_id: RunId,
    cursor: EventCursor,
    mode: DriverMode,
    #[cfg(test)]
    output_bound_override: Option<usize>,
}

#[derive(Clone, Copy)]
enum EventCursor {
    Next(EventSequence),
    Exhausted,
}

enum DriverMode {
    Active,
    Poisoned,
}

#[derive(Debug)]
pub(super) struct SessionDispatch {
    pub(super) messages: Vec<EngineMessage>,
    pub(super) events: Vec<ProtocolEvent>,
}

#[derive(Debug)]
pub(super) enum SessionDispatchError {
    Session {
        error: SessionError,
        frontend_event: ProtocolEvent,
    },
    SequenceOverflow,
    /// Terminal/non-retryable: Session mutated before violating its declared output bound;
    /// this driver does not attempt rollback.
    OutputContractViolation {
        maximum: usize,
        actual: usize,
    },
    DriverPoisoned,
}

impl fmt::Display for SessionDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session { error, .. } => error.fmt(formatter),
            Self::SequenceOverflow => write!(formatter, "protocol event sequence overflow"),
            Self::OutputContractViolation { maximum, actual } => {
                write!(
                    formatter,
                    "session emitted {actual} messages; maximum is {maximum}"
                )
            }
            Self::DriverPoisoned => write!(formatter, "session driver is terminally poisoned"),
        }
    }
}

impl std::error::Error for SessionDispatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Session { error, .. } => Some(error),
            Self::SequenceOverflow
            | Self::OutputContractViolation { .. }
            | Self::DriverPoisoned => None,
        }
    }
}

impl<P> SessionDriver<P>
where
    P: InputPipeline,
{
    pub(super) fn new(session: Session<P>, run_id: RunId) -> Self {
        Self {
            session,
            run_id,
            cursor: EventCursor::Next(EventSequence::new(0)),
            mode: DriverMode::Active,
            #[cfg(test)]
            output_bound_override: None,
        }
    }

    pub(in crate::interactive) fn with_initial_sequence(mut self, sequence: EventSequence) -> Self {
        self.cursor = EventCursor::Next(sequence);
        self
    }

    #[cfg(test)]
    fn with_output_bound_override(mut self, maximum: usize) -> Self {
        self.output_bound_override = Some(maximum);
        self
    }

    pub(super) fn send_at(
        &mut self,
        message: FrontendMessage,
        timestamp: DateTime<Utc>,
    ) -> Result<SessionDispatch, SessionDispatchError> {
        if matches!(self.mode, DriverMode::Poisoned) {
            return Err(SessionDispatchError::DriverPoisoned);
        }

        let maximum_engine_outputs = self.maximum_engine_outputs(&message);
        let frontend_sequence = self.cursor.current()?;
        preflight_last_sequence(frontend_sequence, maximum_engine_outputs + 1)?;
        let frontend = ProtocolEvent::frontend(
            timestamp.clone(),
            self.run_id.clone(),
            frontend_sequence,
            message.clone(),
        );

        let messages = match self.session.handle(message) {
            Ok(messages) => messages,
            Err(error) => {
                self.cursor = EventCursor::after(frontend_sequence);
                return Err(SessionDispatchError::Session {
                    error,
                    frontend_event: frontend,
                });
            }
        };
        if messages.len() > maximum_engine_outputs {
            self.mode = DriverMode::Poisoned;
            return Err(SessionDispatchError::OutputContractViolation {
                maximum: maximum_engine_outputs,
                actual: messages.len(),
            });
        }
        let mut events = Vec::with_capacity(messages.len() + 1);
        events.push(frontend);
        let mut event_sequence = frontend_sequence;
        for engine_message in &messages {
            event_sequence = next_event_sequence(event_sequence)?;
            let event = ProtocolEvent::engine(
                timestamp.clone(),
                self.run_id.clone(),
                event_sequence,
                engine_message.clone(),
            );
            events.push(event);
        }

        self.cursor = EventCursor::after(event_sequence);
        Ok(SessionDispatch { messages, events })
    }

    fn maximum_engine_outputs(&self, message: &FrontendMessage) -> usize {
        #[cfg(test)]
        if let Some(maximum) = self.output_bound_override {
            return maximum;
        }
        maximum_engine_outputs(message)
    }
}

impl EventCursor {
    fn current(self) -> Result<EventSequence, SessionDispatchError> {
        match self {
            Self::Next(sequence) => Ok(sequence),
            Self::Exhausted => Err(SessionDispatchError::SequenceOverflow),
        }
    }

    fn after(sequence: EventSequence) -> Self {
        match sequence.next() {
            Some(next) => Self::Next(next),
            None => Self::Exhausted,
        }
    }
}

fn maximum_engine_outputs(message: &FrontendMessage) -> usize {
    match message {
        FrontendMessage::OpenSession { .. } | FrontendMessage::CloseSession { .. } => 0,
        FrontendMessage::PlatformActionResult { .. } => 1,
        FrontendMessage::KeyCommand { .. } | FrontendMessage::UiCommand { .. } => 2,
    }
}

fn preflight_last_sequence(
    sequence: EventSequence,
    event_count: usize,
) -> Result<EventSequence, SessionDispatchError> {
    let mut last = sequence;
    for _ in 1..event_count {
        last = next_event_sequence(last)?;
    }
    Ok(last)
}

fn next_event_sequence(sequence: EventSequence) -> Result<EventSequence, SessionDispatchError> {
    match sequence.next() {
        Some(next) => Ok(next),
        None => Err(SessionDispatchError::SequenceOverflow),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "session_dispatch_tests.rs"]
mod dispatch_tests;

#[cfg(test)]
#[path = "session_review_tests.rs"]
mod review_tests;

#[cfg(test)]
#[path = "session_cursor_tests.rs"]
mod cursor_tests;

#[cfg(test)]
#[path = "session_bound_tests.rs"]
mod bound_tests;

#[cfg(test)]
#[path = "session_terminal_tests.rs"]
mod terminal_tests;

#[cfg(test)]
#[path = "session_drain_commit_tests.rs"]
mod drain_commit_tests;

#[cfg(test)]
#[path = "session_drain_action_tests.rs"]
mod drain_action_tests;
