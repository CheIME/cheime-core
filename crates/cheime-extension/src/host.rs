use crate::types::{Extension, Filter, Processor, Segmentor, Translator};
use std::collections::HashMap;

#[derive(Default)]
pub struct ExtensionHost {
    extensions: HashMap<String, Box<dyn Extension>>,
}

impl ExtensionHost {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    pub fn register(&mut self, extension: Box<dyn Extension>) {
        self.extensions
            .insert(extension.name().to_owned(), extension);
    }

    pub fn get_processor(&self, name: &str) -> Option<&dyn Processor> {
        self.extensions.get(name)?.processor()
    }

    pub fn get_segmentor(&self, name: &str) -> Option<&dyn Segmentor> {
        self.extensions.get(name)?.segmentor()
    }

    pub fn get_translator(&self, name: &str) -> Option<&dyn Translator> {
        self.extensions.get(name)?.translator()
    }

    pub fn get_filter(&self, name: &str) -> Option<&dyn Filter> {
        self.extensions.get(name)?.filter()
    }

    pub fn has(&self, name: &str) -> bool {
        self.extensions.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExtensionContext, ExtensionError, ExtensionOutput};
    use cheime_model::{Key, KeyEvent, KeyState, Revision, SessionEpoch};

    struct TestProcessor;

    impl Processor for TestProcessor {
        fn process(
            &self,
            ctx: &ExtensionContext,
            _key: &KeyEvent,
        ) -> Result<ExtensionOutput, ExtensionError> {
            Ok(ExtensionOutput::Processor {
                handled: true,
                composition: ctx.composition.to_owned(),
            })
        }
    }

    struct TestExtension {
        name: String,
        processor: Option<TestProcessor>,
    }

    impl Extension for TestExtension {
        fn name(&self) -> &str {
            &self.name
        }
        fn processor(&self) -> Option<&dyn Processor> {
            self.processor.as_ref().map(|p| p as &dyn Processor)
        }
    }

    #[test]
    fn registers_and_queries_extension() {
        let mut host = ExtensionHost::new();
        host.register(Box::new(TestExtension {
            name: "test".into(),
            processor: Some(TestProcessor),
        }));
        assert!(host.has("test"));
        assert!(host.get_processor("test").is_some());
    }

    #[test]
    fn unknown_name_returns_none() {
        let host = ExtensionHost::new();
        assert!(host.get_processor("missing").is_none());
    }

    #[test]
    fn processor_is_invoked() {
        let mut host = ExtensionHost::new();
        host.register(Box::new(TestExtension {
            name: "invoke".into(),
            processor: Some(TestProcessor),
        }));
        let proc = host.get_processor("invoke").unwrap();
        let ctx = ExtensionContext {
            session_epoch: SessionEpoch::new(1),
            revision: Revision::new(2),
            composition: "ni",
            schema_id: "double_pinyin",
        };
        let result = proc
            .process(
                &ctx,
                &KeyEvent {
                    key: Key::Character('n'),
                    state: KeyState::default(),
                },
            )
            .unwrap();
        assert!(matches!(
            result,
            ExtensionOutput::Processor { handled: true, .. }
        ));
    }
}
