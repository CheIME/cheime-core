use super::{SessionDispatchError, SessionDriver};
use crate::interactive::app::{AppState, PlatformActionApplication};
use cheime_pipeline::InputPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

#[derive(Debug)]
pub(in crate::interactive) struct SessionApplicationDispatch {
    pub(super) messages: Vec<EngineMessage>,
    pub(super) events: Vec<crate::interactive::log::ProtocolEvent>,
    pub(super) applications: Vec<PlatformActionApplication>,
}

impl<P> SessionDriver<P>
where
    P: InputPipeline,
{
    /// Sends one frontend message and drains every resulting platform action.
    pub(super) fn send_and_apply_at(
        &mut self,
        message: FrontendMessage,
        timestamp: DateTime<Utc>,
        state: &mut AppState,
    ) -> Result<SessionApplicationDispatch, SessionDispatchError> {
        let initial = self.send_at(message, timestamp.clone())?;
        let mut queue = VecDeque::from(initial.messages);
        let mut messages = Vec::new();
        let mut events = initial.events;
        let mut applications = Vec::new();

        while let Some(engine_message) = queue.pop_front() {
            match &engine_message {
                EngineMessage::CandidateSnapshot { snapshot, .. } => {
                    state.set_snapshot(snapshot.clone());
                }
                EngineMessage::PlatformAction { header, action } => {
                    let application = state.apply_platform_action(action.clone());
                    let acknowledgement = FrontendMessage::PlatformActionResult {
                        header: header.clone(),
                        result: cheime_model::PlatformActionResult {
                            action_id: action.id,
                            outcome: cheime_model::PlatformActionOutcome::Applied,
                        },
                    };
                    let response = self.send_at(acknowledgement, timestamp.clone())?;
                    queue.extend(response.messages);
                    events.extend(response.events);
                    applications.push(application);
                }
                EngineMessage::SessionOpened { .. }
                | EngineMessage::SessionClosed { .. }
                | EngineMessage::ProtocolRejected { .. } => {}
            }
            messages.push(engine_message);
        }

        Ok(SessionApplicationDispatch {
            messages,
            events,
            applications,
        })
    }
}
