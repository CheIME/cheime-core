use cheime_pipeline::InputPipeline;
use cheime_session::Session;
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
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
