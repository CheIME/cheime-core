use cheime_pipeline::InputPipeline;
use cheime_session::Session;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::sync::Arc;

mod drain;

pub(super) struct SessionDriver<P> {
    session: Session<P>,
}

impl<P> SessionDriver<P>
where
    P: InputPipeline,
{
    pub(super) fn new(session: Session<P>) -> Self {
        Self { session }
    }

    pub(super) fn finish_learning(&self, store: &Arc<Mutex<UserStore>>) {
        store.lock().confirm_all_pending();
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
