use crate::{CodeSegment, ComposablePipeline, Filter, Processor, Ranker, Segmentor, Translator};
use cheime_config::schema::{EngineConfig, FilterConfig, ProcessorConfig, SchemaConfig, SegmentorConfig};
use cheime_dictionary::CompiledIndex;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::sync::Arc;

pub use crate::filter::DedupFilter;
pub use crate::processor::DefaultProcessor;
pub use crate::ranker::FrequencyRanker;
pub use crate::segmentor::PinyinSegmentor;
pub use crate::translator::{DictTranslator, PassthroughTranslator, UserDictTranslator};

struct PassthroughSegmentor;
impl Segmentor for PassthroughSegmentor {
    fn segment(&self, c: &str) -> Vec<CodeSegment> {
        if c.is_empty() { return vec![]; }
        vec![CodeSegment { code: c.to_owned(), tag: "passthrough".into() }]
    }
}

pub struct PipelineFactory;

impl PipelineFactory {
    pub fn build(config: &SchemaConfig, user_store: Option<Arc<Mutex<UserStore>>>, dict_index: Option<Arc<CompiledIndex>>, key_mapper: Option<Box<dyn crate::key_mapper::KeyMapper>>) -> Result<ComposablePipeline, BuildError> {
        let mut p = ComposablePipeline::new(
            Self::build_processor(&config.engine)?, Self::build_segmentor(&config.engine)?,
            None,
            Self::build_translators(&config.engine, user_store, dict_index)?,
            Self::build_filters(&config.engine)?, Self::build_ranker());
        if let Some(km) = key_mapper { p = p.with_key_mapper(km); }
        Ok(p)
    }
    fn build_processor(e: &EngineConfig) -> Result<Box<dyn Processor>, BuildError> {
        for p in &e.processors { if matches!(p, ProcessorConfig::AsciiComposer(_) | ProcessorConfig::Speller) { return Ok(Box::new(DefaultProcessor::new())); } }
        Ok(Box::new(DefaultProcessor::new()))
    }
    fn build_segmentor(e: &EngineConfig) -> Result<Box<dyn Segmentor>, BuildError> {
        for s in &e.segmentors { if matches!(s, SegmentorConfig::PinyinSyllable) { return Ok(Box::new(PinyinSegmentor::new())); } }
        Ok(Box::new(PassthroughSegmentor))
    }
    fn build_translators(_e: &EngineConfig, user_store: Option<Arc<Mutex<UserStore>>>, dict_index: Option<Arc<CompiledIndex>>) -> Result<Vec<Box<dyn Translator>>, BuildError> {
        let mut out: Vec<Box<dyn Translator>> = Vec::new();
        if let Some(s) = user_store { out.push(Box::new(UserDictTranslator::new(s))); }
        if let Some(idx) = dict_index { out.push(Box::new(DictTranslator::new("main", idx))); }
        out.push(Box::new(crate::emoji::EmojiTranslator::new()));
        if out.is_empty() { out.push(Box::new(PassthroughTranslator)); }
        Ok(out)
    }
    fn build_filters(e: &EngineConfig) -> Result<Vec<Box<dyn Filter>>, BuildError> {
        let mut out: Vec<Box<dyn Filter>> = Vec::new();
        for f in &e.filters { if matches!(f, FilterConfig::Uniquifier) { out.push(Box::new(DedupFilter::new())); } }
        Ok(out)
    }
    fn build_ranker() -> Box<dyn Ranker> { Box::new(FrequencyRanker::new()) }
}

#[derive(Clone, Debug)]
pub enum BuildError { UnsupportedComponent { component_type: String, pipeline_stage: String }, MissingDictionary { name: String } }
impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { BuildError::UnsupportedComponent { component_type, pipeline_stage } => write!(f, "unsupported '{component_type}' in {pipeline_stage}"), BuildError::MissingDictionary { name } => write!(f, "dictionary '{name}' not found") }
    }
}
impl std::error::Error for BuildError {}

#[cfg(test)]
mod tests {
    use super::*; use crate::InputPipeline;
    use cheime_config::schema::SchemaConfig; use cheime_model::{Key, KeyEvent};
    fn conf(y: &str) -> SchemaConfig { serde_yaml::from_str(y).unwrap() }
    #[test] fn empty_config_works() {
        let p = PipelineFactory::build(&conf("schema_version: 1\nengine: {}\n"), None, None, None).unwrap();
        let r = p.apply("", &KeyEvent { key: Key::Character('n'), state: Default::default() }).unwrap();
        assert_eq!(r.composition, "n");
    }
    #[test] fn user_word_first() {
        let mut s = UserStore::new("t"); s.apply(cheime_user_data::UserEvent::learn_word("t", "qp", "你", "ni"));
        let p = PipelineFactory::build(&conf("schema_version: 1\nengine: {}\n"), Some(Arc::new(Mutex::new(s))), None, None).unwrap();
        let r = p.apply("n", &KeyEvent { key: Key::Character('i'), state: Default::default() }).unwrap();
        assert!(!r.candidates.is_empty()); assert_eq!(r.candidates[0].text, "你");
    }
}
