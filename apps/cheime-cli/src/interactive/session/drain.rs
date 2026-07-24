use super::SessionDriver;
use crate::interactive::app::AppState;
use cheime_pipeline::InputPipeline;
use cheime_protocol::{EngineMessage, FrontendMessage};
use cheime_session::SessionError;
use std::collections::VecDeque;

impl<P> SessionDriver<P>
where
    P: InputPipeline,
{
    pub(in crate::interactive) fn send_and_apply(
        &mut self,
        message: FrontendMessage,
        state: &mut AppState,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        let mut queue = VecDeque::from(self.session.handle(message)?);
        let mut messages = Vec::new();

        while let Some(message) = queue.pop_front() {
            match &message {
                EngineMessage::CandidateSnapshot { snapshot, .. } => {
                    state.set_snapshot(snapshot.clone());
                }
                EngineMessage::PlatformAction { header, action } => {
                    state.apply_platform_action(action);
                    let acknowledgement = FrontendMessage::PlatformActionResult {
                        header: header.clone(),
                        result: cheime_model::PlatformActionResult {
                            action_id: action.id,
                            outcome: cheime_model::PlatformActionOutcome::Applied,
                        },
                    };
                    queue.extend(self.session.handle(acknowledgement)?);
                }
                EngineMessage::SessionOpened { .. }
                | EngineMessage::SessionClosed { .. }
                | EngineMessage::ProtocolRejected { .. } => {}
            }
            messages.push(message);
        }

        Ok(messages)
    }
}
