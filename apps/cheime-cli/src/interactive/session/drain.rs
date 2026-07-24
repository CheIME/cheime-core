use super::{SessionDispatchError, SessionDriver};
use crate::interactive::app::{AppState, PlatformActionApplication};
use cheime_pipeline::InputPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage};
use cheime_user_data::UserStore;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// The interactive state and learning store mutated by one drain.
pub(in crate::interactive) struct SessionApplicationContext<'a> {
    state: &'a mut AppState,
    store: &'a mut UserStore,
}

impl<'a> SessionApplicationContext<'a> {
    pub(in crate::interactive) fn new(state: &'a mut AppState, store: &'a mut UserStore) -> Self {
        Self { state, store }
    }
}

#[derive(Debug)]
pub(in crate::interactive) struct SessionApplicationDispatch {
    #[allow(dead_code)]
    pub(super) messages: Vec<EngineMessage>,
    #[allow(dead_code)]
    pub(super) events: Vec<crate::interactive::log::ProtocolEvent>,
    #[allow(dead_code)]
    pub(super) applications: Vec<PlatformActionApplication>,
}

impl<P> SessionDriver<P>
where
    P: InputPipeline,
{
    /// Sends one frontend message and drains every resulting platform action.
    pub(in crate::interactive) fn send_and_apply_at(
        &mut self,
        message: FrontendMessage,
        timestamp: DateTime<Utc>,
        context: SessionApplicationContext<'_>,
    ) -> Result<SessionApplicationDispatch, SessionDispatchError> {
        let SessionApplicationContext { state, store } = context;
        let initial = self.send_at(message, timestamp)?;
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
                    let response = self.send_at(acknowledgement, timestamp)?;
                    queue.extend(response.messages);
                    events.extend(response.events);
                    match &application {
                        PlatformActionApplication::NoDocumentChange { .. } => {}
                        PlatformActionApplication::Committed { text, .. } => {
                            store.commit_pending(text, "", "quanpin");
                        }
                    }
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
