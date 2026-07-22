use crate::{CodeSegment, ComposablePipeline, Filter, Processor, Ranker, Segmentor, Translator};
use crate::simplifier::{Conversion, SimplifierFilter};
use cheime_config::schema::{EngineConfig, FilterConfig, SchemaConfig, SegmentorConfig};
use cheime_dictionary::CompiledIndex;
use cheime_user_data::UserStore;
use parking_lot::Mutex;
use std::sync::Arc;

pub use crate::filter::DedupFilter;
pub use crate::processor::DefaultProcessor;
pub use crate::ranker::UnifiedRanker;
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
            Self::build_processor(config)?, Self::build_segmentor(&config.engine)?,
            Self::build_normalizer(&config.engine),
            Self::build_translators(&config.engine, user_store, dict_index)?,
            Self::build_filters(&config.engine)?, Self::build_ranker());
        if let Some(km) = key_mapper { p = p.with_key_mapper(km); }
        Ok(p)
    }
    fn build_processor(config: &SchemaConfig) -> Result<Box<dyn Processor>, BuildError> {
        let inner: Box<dyn Processor> = Box::new(DefaultProcessor::new());
        if let Some(ref punct) = config.punctuator {
            return Ok(Box::new(crate::punctuator::PunctProcessor::new(punct, false, inner)));
        }
        Ok(inner)
    }
    fn build_segmentor(e: &EngineConfig) -> Result<Box<dyn Segmentor>, BuildError> {
        for s in &e.segmentors { if matches!(s, SegmentorConfig::PinyinSyllable) { return Ok(Box::new(PinyinSegmentor::new())); } }
        Ok(Box::new(PassthroughSegmentor))
    }
    fn build_normalizer(e: &EngineConfig) -> Option<Box<dyn crate::normalizer::CodeNormalizer>> {
        use crate::normalizer::{AbbreviationNormalizer, CompositeNormalizer, FuzzyNormalizer};
        use cheime_config::schema::SegmentorConfig;

        let mut normalizers: Vec<Box<dyn crate::normalizer::CodeNormalizer>> = Vec::new();

        // Abbreviation normalizer (auto-enabled for pinyin segmentor)
        if e.segmentors.iter().any(|s| matches!(s, SegmentorConfig::PinyinSyllable)) {
            normalizers.push(Box::new(AbbreviationNormalizer::new()));
        }

        // Fuzzy normalizer (configurable)
        if let Some(ref fuzzy) = e.fuzzy_pinyin {
            if fuzzy.enabled {
                if fuzzy.rules.is_empty() {
                    normalizers.push(Box::new(FuzzyNormalizer::standard()));
                } else {
                    normalizers.push(Box::new(FuzzyNormalizer::from_rules(&fuzzy.rules)));
                }
            }
        }

        match normalizers.len() {
            0 => None,
            1 => Some(normalizers.into_iter().next().unwrap()),
            _ => Some(Box::new(CompositeNormalizer::new(normalizers))),
        }
    }

    fn build_translators(e: &EngineConfig, user_store: Option<Arc<Mutex<UserStore>>>, dict_index: Option<Arc<CompiledIndex>>) -> Result<Vec<Box<dyn Translator>>, BuildError> {
        use cheime_config::schema::TranslatorConfig;
        let mut out: Vec<Box<dyn Translator>> = Vec::new();

        for tc in &e.translators {
            match tc {
                TranslatorConfig::Dict(_) | TranslatorConfig::Table(_) => {
                    if let Some(ref idx) = dict_index {
                        out.push(Box::new(DictTranslator::new("main", Arc::clone(idx))));
                    }
                }
                TranslatorConfig::Emoji(ec) => {
                    let path = std::path::Path::new(&ec.emoji_data);
                    out.push(Box::new(crate::emoji::EmojiTranslator::from_file(path)));
                }
                TranslatorConfig::Script(_) | TranslatorConfig::Lua(_) => {
                    // Not yet implemented — skip
                }
                _ => {}
            }
        }

        // Fallback: if no translators configured, use defaults
        if out.is_empty() {
            if let Some(s) = user_store { out.push(Box::new(UserDictTranslator::new(s))); }
            if let Some(idx) = dict_index { out.push(Box::new(DictTranslator::new("main", idx))); }
            out.push(Box::new(crate::emoji::EmojiTranslator::from_file(std::path::Path::new("data/emoji.txt"))));
            if out.is_empty() { out.push(Box::new(PassthroughTranslator)); }
        }
        Ok(out)
    }
    fn build_filters(e: &EngineConfig) -> Result<Vec<Box<dyn Filter>>, BuildError> {
        let mut out: Vec<Box<dyn Filter>> = Vec::new();
        for f in &e.filters {
            match f {
                FilterConfig::Uniquifier => { out.push(Box::new(DedupFilter::new())); }
                FilterConfig::Simplifier(cfg) => {
                    let direction = match cfg.option_name.as_deref() {
                        Some("s2t") | Some("simplified_to_traditional") | Some("s2t.json") => Conversion::S2T,
                        Some("t2s") | Some("traditional_to_simplified") | Some("t2s.json") => Conversion::T2S,
                        _ => return Err(BuildError::UnsupportedComponent { component_type: format!("simplifier({:?})", cfg.option_name), pipeline_stage: "filter".into() }),
                    };
                    let filter = match &cfg.opencc_config {
                        Some(path) => {
                            let full = std::path::Path::new(path);
                            SimplifierFilter::from_file(full, direction, true)
                                .map_err(|e| BuildError::MissingDictionary { name: e.to_string() })?
                        }
                        None => {
                            return Err(BuildError::UnsupportedComponent { component_type: "simplifier(no opencc_config)".into(), pipeline_stage: "filter".into() })
                        }
                    };
                    out.push(Box::new(filter));
                }
                _ => { /* skip unknown filters */ }
            }
        }
        Ok(out)
    }
    fn build_ranker() -> Box<dyn Ranker> {
        Box::new(UnifiedRanker::new(Default::default()))
    }
}
#[derive(Clone, Debug)]
pub enum BuildError {
    UnsupportedComponent { component_type: String, pipeline_stage: String },
    MissingDictionary { name: String },
    SimplifierLoad { error: String },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedComponent { component_type, pipeline_stage } => write!(f, "unsupported '{component_type}' in {pipeline_stage}"),
            Self::MissingDictionary { name } => write!(f, "dictionary '{name}' not found"),
            Self::SimplifierLoad { error } => write!(f, "simplifier load failed: {error}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl BuildError {
    /// Convert to a structured DiagnosticError for reporting.
    pub fn to_diagnostic(&self) -> cheime_diagnostics::DiagnosticError {
        match self {
            Self::UnsupportedComponent { component_type, pipeline_stage } => {
                cheime_diagnostics::DiagnosticError::component_build(
                    pipeline_stage,
                    format!("unsupported component: {component_type}"),
                )
            }
            Self::MissingDictionary { name } => {
                cheime_diagnostics::DiagnosticError::new(
                    "E-DICT-MISSING", cheime_diagnostics::Severity::ComponentInit,
                    format!("Dictionary '{name}' is required but not found"),
                )
                .with_component(name)
            }
            Self::SimplifierLoad { error } => {
                cheime_diagnostics::DiagnosticError::new(
                    "E-SIMPLIFIER-LOAD", cheime_diagnostics::Severity::ComponentInit,
                    error.clone(),
                )
            }
        }
    }
}

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

    #[test]
    fn snapshot_nihao_with_dict() {
        let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
        let body = if let Some(p) = raw.find("\n...\n") { &raw[p + 5..] } else { raw };
        let cols = &[cheime_dictionary::DictColumn::Text, cheime_dictionary::DictColumn::Code, cheime_dictionary::DictColumn::Weight];
        let entries = cheime_dictionary::parse_body(body, cols).unwrap();
        let idx = Arc::new(cheime_dictionary::CompiledIndex::build(entries, cheime_model::DeploymentGeneration::new(1)));
        let p = PipelineFactory::build(
            &conf("schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n"), None, Some(idx), None,
        ).unwrap();

        let mut comp = String::new();
        for c in "nihao".chars() {
            let r = p.apply(&comp, &KeyEvent { key: Key::Character(c), state: Default::default() }).unwrap();
            comp = r.composition;
            if comp == "nihao" {
                assert!(r.candidates.len() >= 3, "expected at least 3 candidates for nihao, got {:?}", r.candidates.iter().map(|c| &c.text).collect::<Vec<_>>());
                assert_eq!(r.candidates[0].text, "你好");
                assert!(r.candidates.iter().any(|c| c.is_emoji), "should have emoji candidate");
            }
        }
    }
}
