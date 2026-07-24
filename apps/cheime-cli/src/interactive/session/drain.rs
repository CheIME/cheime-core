use super::SessionDriver;
use crate::interactive::app::AppState;
use cheime_pipeline::{InputPipeline, Segmentor, segmentor::PinyinSegmentor};
use cheime_protocol::{EngineMessage, FrontendMessage};
use cheime_session::SessionError;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

impl<P> SessionDriver<P>
where
    P: InputPipeline,
{
    pub(in crate::interactive) fn send_and_apply(
        &mut self,
        message: FrontendMessage,
        state: &mut AppState,
        store: &Arc<Mutex<UserStore>>,
    ) -> Result<Vec<EngineMessage>, SessionError> {
        let mut queue = VecDeque::from(self.session.handle(message)?);
        let mut messages = Vec::new();

        while let Some(message) = queue.pop_front() {
            match &message {
                EngineMessage::CandidateSnapshot { snapshot, .. } => {
                    state.set_snapshot(snapshot.clone());
                }
                EngineMessage::PlatformAction { header, action } => {
                    let committed_text = state.apply_platform_action(action);
                    let learning_code = committed_text
                        .as_ref()
                        .map(|_| normalized_pinyin(self.session.composition()));
                    let acknowledgement = FrontendMessage::PlatformActionResult {
                        header: header.clone(),
                        result: cheime_model::PlatformActionResult {
                            action_id: action.id,
                            outcome: cheime_model::PlatformActionOutcome::Applied,
                        },
                    };
                    queue.extend(self.session.handle(acknowledgement)?);
                    if let Some((text, code)) = committed_text.zip(learning_code)
                        && !code.is_empty()
                    {
                        store.lock().commit_pending(&text, &code, "quanpin");
                    }
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

pub(super) fn normalized_pinyin(composition: &str) -> String {
    PinyinSegmentor::new()
        .segment(composition)
        .into_iter()
        .map(|segment| segment.code)
        .collect::<Vec<_>>()
        .join(" ")
}
