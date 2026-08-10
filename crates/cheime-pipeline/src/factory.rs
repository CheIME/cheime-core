use crate::language_model::{LanguageModel, NullLanguageModel};
use crate::learning::LearningService;
use crate::segmentation::{InputSpan, SegmentationGraph, SyllableEdge, SyllableKind};
use crate::simplifier::{Conversion, SimplifierFilter};
use crate::{ComposablePipeline, Filter, Processor, Ranker, Segmentor, Translator};
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
    fn segment(&self, c: &str) -> SegmentationGraph {
        let mut graph = SegmentationGraph::new(c.len());
        if !c.is_empty() {
            graph.add_edge(SyllableEdge {
                span: InputSpan::new(0, c.len()),
                raw: c.to_owned(),
                canonical: c.to_owned(),
                kind: SyllableKind::Raw,
            });
        }
        graph
    }
}

pub struct PipelineFactory;

impl PipelineFactory {
    pub fn build(
        config: &SchemaConfig,
        user_store: Option<Arc<Mutex<UserStore>>>,
        dict_index: Option<Arc<CompiledIndex>>,
    ) -> Result<ComposablePipeline, BuildError> {
        let learning = user_store.map(LearningService::production).map(Arc::new);
        Self::build_with_learning(config, learning, dict_index)
    }

    /// Build a pipeline with an explicitly supplied language model.
    ///
    /// Existing callers should continue using [`Self::build`]; it installs a
    /// zero-cost model and therefore preserves historical ranking.
    pub fn build_with_language_model(
        config: &SchemaConfig,
        user_store: Option<Arc<Mutex<UserStore>>>,
        dict_index: Option<Arc<CompiledIndex>>,
        language_model: Arc<dyn LanguageModel>,
    ) -> Result<ComposablePipeline, BuildError> {
        let learning = user_store.map(LearningService::production).map(Arc::new);
        Self::build_with_learning_and_language_model(config, learning, dict_index, language_model)
    }

    pub fn build_with_learning(
        config: &SchemaConfig,
        learning: Option<Arc<LearningService>>,
        dict_index: Option<Arc<CompiledIndex>>,
    ) -> Result<ComposablePipeline, BuildError> {
        Self::build_with_learning_and_language_model(
            config,
            learning,
            dict_index,
            Arc::new(NullLanguageModel),
        )
    }

    pub fn build_with_learning_and_language_model(
        config: &SchemaConfig,
        learning: Option<Arc<LearningService>>,
        dict_index: Option<Arc<CompiledIndex>>,
        language_model: Arc<dyn LanguageModel>,
    ) -> Result<ComposablePipeline, BuildError> {
        let user_store = learning.as_ref().map(|service| service.store());
        let mut p = ComposablePipeline::new(
            Self::build_processor(config)?,
            Self::build_segmentor(&config.engine)?,
            Self::build_normalizer(&config.engine),
            Self::build_translators(&config.engine, user_store, dict_index, language_model)?,
            Self::build_filters(&config.engine)?,
            Self::build_ranker(),
        )
        .with_schema_id(
            config
                .schema
                .as_ref()
                .and_then(|schema| schema.schema_id.clone())
                .unwrap_or_else(|| String::from("default")),
        );
        if let Some(learning) = learning {
            p = p.with_learning(learning);
        }
        Ok(p)
    }
    fn build_processor(config: &SchemaConfig) -> Result<Box<dyn Processor>, BuildError> {
        let inner: Box<dyn Processor> = Box::new(DefaultProcessor::new());
        if let Some(ref punct) = config.punctuator {
            return Ok(Box::new(crate::punctuator::PunctProcessor::new(
                punct, false, inner,
            )));
        }
        Ok(inner)
    }
    fn build_segmentor(e: &EngineConfig) -> Result<Box<dyn Segmentor>, BuildError> {
        use crate::double_pinyin::{
            CodeConfusionModel, CompiledDoublePinyinTable, DoublePinyinSegmentor,
            KeyboardMistouchModel,
        };
        use cheime_config::schema::InputConfig;

        match &e.input {
            Some(InputConfig::QuanPin(input)) => {
                let mut segmentor = PinyinSegmentor::new();
                if let Some(correction) = &input.spelling_correction {
                    segmentor =
                        segmentor.with_correction(crate::segmentor::PinyinCorrectionOptions {
                            enabled: correction.enabled,
                            max_edit_distance: correction.max_edit_distance,
                            max_candidates_per_start: correction.max_candidates_per_start,
                            edit_penalty: correction.edit_penalty,
                        });
                }
                Ok(Box::new(segmentor))
            }
            Some(InputConfig::DoublePinyin(input)) => {
                let table = CompiledDoublePinyinTable::from_scheme_config(&input.scheme)
                    .map_err(|message| BuildError::InvalidDoublePinyinScheme { message })?;
                let mut segmentor = DoublePinyinSegmentor::new(table);
                if let Some(mistouch) = &input.keyboard_mistouch {
                    if mistouch.enabled {
                        if mistouch.layout != "qwerty" {
                            return Err(BuildError::UnsupportedKeyboardLayout {
                                layout: mistouch.layout.clone(),
                            });
                        }
                        segmentor =
                            segmentor.with_keyboard(KeyboardMistouchModel::qwerty(mistouch.cost));
                    }
                }
                if let Some(confusion) = &input.code_confusion {
                    if confusion.enabled {
                        let rules: Vec<(String, String, Option<i64>)> = confusion
                            .rules
                            .iter()
                            .map(|rule| (rule.from.clone(), rule.to.clone(), rule.cost))
                            .collect();
                        segmentor = segmentor.with_confusion(
                            CodeConfusionModel::from_rules(confusion.cost, &rules).map_err(
                                |message| BuildError::InvalidCodeConfusionRule { message },
                            )?,
                        );
                    }
                }
                Ok(Box::new(segmentor))
            }
            None => {
                // Legacy path: schemas without `engine.input`.
                for s in &e.segmentors {
                    if matches!(s, SegmentorConfig::PinyinSyllable) {
                        let mut segmentor = PinyinSegmentor::new();
                        if let Some(correction) = &e.pinyin_correction {
                            segmentor = segmentor.with_correction(
                                crate::segmentor::PinyinCorrectionOptions {
                                    enabled: correction.enabled,
                                    max_edit_distance: correction.max_edit_distance,
                                    max_candidates_per_start: correction.max_candidates_per_start,
                                    edit_penalty: correction.edit_penalty,
                                },
                            );
                        }
                        return Ok(Box::new(segmentor));
                    }
                }
                Ok(Box::new(PassthroughSegmentor))
            }
        }
    }
    fn build_normalizer(e: &EngineConfig) -> Option<Box<dyn crate::normalizer::CodeNormalizer>> {
        use crate::normalizer::{AbbreviationNormalizer, CompositeNormalizer, FuzzyNormalizer};
        use cheime_config::schema::{InputConfig, SegmentorConfig};

        let mut normalizers: Vec<Box<dyn crate::normalizer::CodeNormalizer>> = Vec::new();

        let is_quanpin = match &e.input {
            Some(InputConfig::QuanPin(_)) => true,
            Some(InputConfig::DoublePinyin(_)) => false,
            None => e
                .segmentors
                .iter()
                .any(|s| matches!(s, SegmentorConfig::PinyinSyllable)),
        };

        // Abbreviation (简拼) only applies to quanpin: double-pinyin single
        // keys are already Incomplete edges with the initial as canonical
        // (e.g. `v` → "zh"), which gives prefix completion for free.
        if is_quanpin {
            normalizers.push(Box::new(AbbreviationNormalizer::new()));
        }

        // Fuzzy pinyin normalization is shared: it operates on canonical
        // syllables, after the input scheme has been decoded.
        if let Some(fuzzy) = &e.fuzzy_pinyin {
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
            1 => normalizers.pop(),
            _ => Some(Box::new(CompositeNormalizer::new(normalizers))),
        }
    }

    fn build_translators(
        e: &EngineConfig,
        user_store: Option<Arc<Mutex<UserStore>>>,
        dict_index: Option<Arc<CompiledIndex>>,
        language_model: Arc<dyn LanguageModel>,
    ) -> Result<Vec<Box<dyn Translator>>, BuildError> {
        use cheime_config::schema::TranslatorConfig;
        let mut out: Vec<Box<dyn Translator>> = Vec::new();
        let mut has_dictionary = false;

        for tc in &e.translators {
            match tc {
                TranslatorConfig::Dict(config) => {
                    if let Some(ref idx) = dict_index {
                        let mut translator = DictTranslator::new("main", Arc::clone(idx))
                            .with_options(crate::decoder::DecoderOptions {
                                enable_completion: config.enable_completion,
                                enable_sentence: config.enable_sentence,
                            })
                            .with_language_model(Arc::clone(&language_model));
                        if let Some(store) = user_store.as_ref() {
                            translator = translator.with_user_store(Arc::clone(store));
                        }
                        out.push(Box::new(translator));
                        has_dictionary = true;
                    }
                }
                TranslatorConfig::Table(config) => {
                    if let Some(ref idx) = dict_index {
                        let mut translator = DictTranslator::new("main", Arc::clone(idx))
                            .with_options(crate::decoder::DecoderOptions {
                                enable_completion: config.enable_completion,
                                enable_sentence: config.enable_sentence,
                            })
                            .with_language_model(Arc::clone(&language_model));
                        if let Some(store) = user_store.as_ref() {
                            translator = translator.with_user_store(Arc::clone(store));
                        }
                        out.push(Box::new(translator));
                        has_dictionary = true;
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

        // Fallback: if no translators are configured, add the default static
        // dictionary and emoji sources in addition to the user lexicon.
        if e.translators.is_empty() {
            if let Some(idx) = dict_index {
                let mut translator = DictTranslator::new("main", idx)
                    .with_language_model(Arc::clone(&language_model));
                if let Some(store) = user_store.as_ref() {
                    translator = translator.with_user_store(Arc::clone(store));
                }
                out.push(Box::new(translator));
                has_dictionary = true;
            }
            out.push(Box::new(crate::emoji::EmojiTranslator::from_file(
                std::path::Path::new("data/emoji.txt"),
            )));
        }
        if !has_dictionary {
            if let Some(store) = user_store {
                out.insert(
                    0,
                    Box::new(
                        UserDictTranslator::new(store)
                            .with_language_model(Arc::clone(&language_model)),
                    ),
                );
            }
        }
        if out.is_empty() {
            out.push(Box::new(PassthroughTranslator));
        }
        Ok(out)
    }
    fn build_filters(e: &EngineConfig) -> Result<Vec<Box<dyn Filter>>, BuildError> {
        let mut out: Vec<Box<dyn Filter>> = Vec::new();
        for f in &e.filters {
            match f {
                FilterConfig::Uniquifier => {
                    out.push(Box::new(DedupFilter::new()));
                }
                FilterConfig::Simplifier(cfg) => {
                    let direction = match cfg.option_name.as_deref() {
                        Some("s2t") | Some("simplified_to_traditional") | Some("s2t.json") => {
                            Conversion::S2T
                        }
                        Some("t2s") | Some("traditional_to_simplified") | Some("t2s.json") => {
                            Conversion::T2S
                        }
                        _ => {
                            return Err(BuildError::UnsupportedComponent {
                                component_type: format!("simplifier({:?})", cfg.option_name),
                                pipeline_stage: "filter".into(),
                            });
                        }
                    };
                    let filter = match &cfg.opencc_config {
                        Some(path) => {
                            let full = std::path::Path::new(path);
                            SimplifierFilter::from_file(full, direction, true).map_err(|e| {
                                BuildError::MissingDictionary {
                                    name: e.to_string(),
                                }
                            })?
                        }
                        None => {
                            return Err(BuildError::UnsupportedComponent {
                                component_type: "simplifier(no opencc_config)".into(),
                                pipeline_stage: "filter".into(),
                            });
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
    UnsupportedComponent {
        component_type: String,
        pipeline_stage: String,
    },
    MissingDictionary {
        name: String,
    },
    SimplifierLoad {
        error: String,
    },
    InvalidDoublePinyinScheme {
        message: String,
    },
    UnsupportedKeyboardLayout {
        layout: String,
    },
    InvalidCodeConfusionRule {
        message: String,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedComponent {
                component_type,
                pipeline_stage,
            } => write!(f, "unsupported '{component_type}' in {pipeline_stage}"),
            Self::MissingDictionary { name } => write!(f, "dictionary '{name}' not found"),
            Self::SimplifierLoad { error } => write!(f, "simplifier load failed: {error}"),
            Self::InvalidDoublePinyinScheme { message } => {
                write!(f, "invalid double-pinyin scheme: {message}")
            }
            Self::UnsupportedKeyboardLayout { layout } => {
                write!(
                    f,
                    "unsupported keyboard layout '{layout}' (only 'qwerty' is available)"
                )
            }
            Self::InvalidCodeConfusionRule { message } => {
                write!(f, "invalid code-confusion rule: {message}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl BuildError {
    /// Convert to a structured DiagnosticError for reporting.
    pub fn to_diagnostic(&self) -> cheime_diagnostics::DiagnosticError {
        match self {
            Self::UnsupportedComponent {
                component_type,
                pipeline_stage,
            } => cheime_diagnostics::DiagnosticError::component_build(
                pipeline_stage,
                format!("unsupported component: {component_type}"),
            ),
            Self::MissingDictionary { name } => cheime_diagnostics::DiagnosticError::new(
                "E-DICT-MISSING",
                cheime_diagnostics::Severity::ComponentInit,
                format!("Dictionary '{name}' is required but not found"),
            )
            .with_component(name),
            Self::SimplifierLoad { error } => cheime_diagnostics::DiagnosticError::new(
                "E-SIMPLIFIER-LOAD",
                cheime_diagnostics::Severity::ComponentInit,
                error.clone(),
            ),
            Self::InvalidDoublePinyinScheme { message } => {
                cheime_diagnostics::DiagnosticError::new(
                    "E-SCHEME-INVALID",
                    cheime_diagnostics::Severity::ComponentInit,
                    format!("Invalid double-pinyin scheme: {message}"),
                )
            }
            Self::UnsupportedKeyboardLayout { layout } => cheime_diagnostics::DiagnosticError::new(
                "E-KEYBOARD-LAYOUT",
                cheime_diagnostics::Severity::ComponentInit,
                format!("Unsupported keyboard layout '{layout}' (only 'qwerty' is available)"),
            ),
            Self::InvalidCodeConfusionRule { message } => cheime_diagnostics::DiagnosticError::new(
                "E-CONFUSION-RULE",
                cheime_diagnostics::Severity::ComponentInit,
                format!("Invalid code-confusion rule: {message}"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InputPipeline;
    use cheime_config::schema::SchemaConfig;
    use cheime_dictionary::{CompiledIndex, DictEntry};
    use cheime_model::DeploymentGeneration;
    use cheime_model::{Key, KeyEvent};
    fn conf(y: &str) -> SchemaConfig {
        serde_yaml::from_str(y).unwrap()
    }
    fn key(ch: char) -> KeyEvent {
        KeyEvent {
            key: Key::Character(ch),
            state: Default::default(),
        }
    }

    fn tiny_index() -> Arc<CompiledIndex> {
        Arc::new(CompiledIndex::build(
            vec![
                DictEntry {
                    text: "中".into(),
                    code: "zhong".into(),
                    weight: Some(100),
                    stem: None,
                },
                DictEntry {
                    text: "国".into(),
                    code: "guo".into(),
                    weight: Some(100),
                    stem: None,
                },
                DictEntry {
                    text: "中国".into(),
                    code: "zhong guo".into(),
                    weight: Some(500),
                    stem: None,
                },
            ],
            DeploymentGeneration::new(1),
        ))
    }
    #[test]
    fn empty_config_works() {
        let p =
            PipelineFactory::build(&conf("schema_version: 1\nengine: {}\n"), None, None).unwrap();
        let r = p
            .apply(
                "",
                &KeyEvent {
                    key: Key::Character('n'),
                    state: Default::default(),
                },
            )
            .unwrap();
        assert_eq!(r.composition, "n");
    }
    #[test]
    fn user_word_first() {
        let mut s = UserStore::new("t");
        s.apply(cheime_user_data::UserEvent::learn_word(
            "t", "qp", "你", "ni",
        ));
        let p = PipelineFactory::build(
            &conf("schema_version: 1\nengine: {}\n"),
            Some(Arc::new(Mutex::new(s))),
            None,
        )
        .unwrap();
        let r = p
            .apply(
                "n",
                &KeyEvent {
                    key: Key::Character('i'),
                    state: Default::default(),
                },
            )
            .unwrap();
        assert!(!r.candidates.is_empty());
        assert_eq!(r.candidates[0].text, "你");
    }

    #[test]
    fn configured_dictionary_does_not_suppress_user_words() {
        let mut store = UserStore::new("test");
        store.apply(cheime_user_data::UserEvent::learn_word(
            "test", "qp", "旎", "ni",
        ));
        let index = Arc::new(CompiledIndex::build(
            vec![cheime_dictionary::DictEntry {
                text: String::from("你"),
                code: String::from("ni"),
                weight: Some(100),
                stem: None,
            }],
            cheime_model::DeploymentGeneration::new(1),
        ));
        let pipeline = PipelineFactory::build(&conf(
            "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  translators:\n    - type: dict\n      dictionary: main\n",
        ),
        Some(Arc::new(Mutex::new(store))),
        Some(index))
        .unwrap();
        let update = pipeline
            .apply(
                "n",
                &KeyEvent {
                    key: Key::Character('i'),
                    state: Default::default(),
                },
            )
            .unwrap();
        assert_eq!(update.candidates[0].text, "旎");
        assert_eq!(update.candidates[0].source, "user:learned");
    }

    #[test]
    fn accidental_learning_does_not_override_common_dictionary_words() {
        let mut store = UserStore::new("test");
        store.apply(cheime_user_data::UserEvent::learn_word(
            "test", "qp", "字级", "zi ji",
        ));
        store.apply(cheime_user_data::UserEvent::learn_word(
            "test", "qp", "孑孓", "jie jue",
        ));
        store.apply(cheime_user_data::UserEvent::learn_word(
            "test", "qp", "是的", "shi de",
        ));
        let index = Arc::new(CompiledIndex::build(
            vec![
                cheime_dictionary::DictEntry {
                    text: String::from("自己"),
                    code: String::from("zi ji"),
                    weight: Some(507_135),
                    stem: None,
                },
                cheime_dictionary::DictEntry {
                    text: String::from("解决"),
                    code: String::from("jie jue"),
                    weight: Some(501_191),
                    stem: None,
                },
                cheime_dictionary::DictEntry {
                    text: String::from("是"),
                    code: String::from("shi"),
                    weight: Some(31_422_712),
                    stem: None,
                },
            ],
            cheime_model::DeploymentGeneration::new(1),
        ));
        let pipeline = PipelineFactory::build(&conf(
            "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  translators:\n    - type: dict\n      dictionary: main\n",
        ),
        Some(Arc::new(Mutex::new(store))),
        Some(index))
        .unwrap();

        let ziji = pipeline.refresh("ziji").unwrap();
        let jiejue = pipeline.refresh("jiejue").unwrap();
        let sh = pipeline.refresh("sh").unwrap();

        assert_eq!(ziji[0].text, "自己");
        assert!(ziji.iter().any(|candidate| candidate.text == "字级"));
        assert_eq!(jiejue[0].text, "解决");
        assert!(jiejue.iter().any(|candidate| candidate.text == "孑孓"));
        assert_eq!(sh[0].text, "是");
        assert!(sh.iter().any(|candidate| candidate.text == "是的"));
    }

    #[test]
    fn learned_and_static_lexemes_compose_in_one_word_graph() {
        let mut store = UserStore::new("test");
        store.apply(cheime_user_data::UserEvent::learn_word(
            "test", "qp", "旎", "ni",
        ));
        let index = Arc::new(CompiledIndex::build(
            vec![cheime_dictionary::DictEntry {
                text: String::from("皓"),
                code: String::from("hao"),
                weight: Some(100),
                stem: None,
            }],
            cheime_model::DeploymentGeneration::new(1),
        ));
        let pipeline = PipelineFactory::build(&conf(
            "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  translators:\n    - type: dict\n      dictionary: main\n",
        ),
        Some(Arc::new(Mutex::new(store))),
        Some(index))
        .unwrap();

        let candidates = pipeline.refresh("nihao").unwrap();
        let composed = candidates
            .iter()
            .find(|candidate| candidate.text == "旎皓")
            .expect("mixed user/static sentence");
        assert_eq!(composed.lexemes.len(), 2);
        assert_eq!(composed.lexemes[0].source, "user:learned");
    }

    #[test]
    fn injected_language_model_reaches_dictionary_decoder() {
        use crate::language_model::BackoffNgramModel;

        let index = Arc::new(CompiledIndex::build(
            vec![
                cheime_dictionary::DictEntry {
                    text: String::from("甲"),
                    code: String::from("ni"),
                    weight: Some(100),
                    stem: None,
                },
                cheime_dictionary::DictEntry {
                    text: String::from("乙"),
                    code: String::from("ni"),
                    weight: Some(100),
                    stem: None,
                },
                cheime_dictionary::DictEntry {
                    text: String::from("丙"),
                    code: String::from("hao"),
                    weight: Some(100),
                    stem: None,
                },
                cheime_dictionary::DictEntry {
                    text: String::from("丁"),
                    code: String::from("hao"),
                    weight: Some(100),
                    stem: None,
                },
            ],
            cheime_model::DeploymentGeneration::new(1),
        ));
        let model = Arc::new(BackoffNgramModel::new(0).with_bigram("甲", "丁", 1_000));
        let pipeline = PipelineFactory::build_with_language_model(
            &conf("schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n"),
            None,
            Some(index),
            model,
        )
        .unwrap();

        let candidates = pipeline.refresh("nihao").unwrap();
        assert_eq!(candidates[0].display.text, "甲丁");
    }

    #[test]
    fn configured_correction_decodes_typo_without_rewriting_composition() {
        let index = Arc::new(CompiledIndex::build(
            vec![cheime_dictionary::DictEntry {
                text: String::from("什么"),
                code: String::from("shen me"),
                weight: Some(200),
                stem: None,
            }],
            cheime_model::DeploymentGeneration::new(1),
        ));
        let pipeline = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  pinyin_correction:\n    enabled: true\n    max_candidates_per_start: 64\n",
            ),
            None,
            Some(index),
        )
        .unwrap();

        let update = pipeline
            .apply(
                "shene",
                &KeyEvent {
                    key: Key::Character('m'),
                    state: Default::default(),
                },
            )
            .unwrap();

        assert_eq!(update.composition, "shenem");
        assert!(
            update
                .candidates
                .iter()
                .any(|candidate| candidate.display.text == "什么")
        );
    }

    fn rime_body(raw: &str) -> &str {
        raw.find("\n...\r\n")
            .map(|start| &raw[start + 6..])
            .or_else(|| raw.find("\n...\n").map(|start| &raw[start + 5..]))
            .unwrap_or(raw)
    }

    #[test]
    fn rime_body_skips_lf_and_crlf_headers() {
        let lf_body = rime_body("---\nname: base\n...\n你好\tni hao\t1\n");
        let crlf_body = rime_body("---\r\nname: base\r\n...\r\n你好\tni hao\t1\r\n");

        assert_eq!(lf_body, "你好\tni hao\t1\n");
        assert_eq!(crlf_body, "你好\tni hao\t1\r\n");
    }

    #[test]
    fn snapshot_nihao_with_dict() {
        let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
        let body = rime_body(raw);
        let cols = &[
            cheime_dictionary::DictColumn::Text,
            cheime_dictionary::DictColumn::Code,
            cheime_dictionary::DictColumn::Weight,
        ];
        let entries = cheime_dictionary::parse_body(body, cols).unwrap();
        let idx = Arc::new(cheime_dictionary::CompiledIndex::build(
            entries,
            cheime_model::DeploymentGeneration::new(1),
        ));
        let p = PipelineFactory::build(
            &conf("schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n"),
            None,
            Some(idx),
        )
        .unwrap();

        let mut comp = String::new();
        for c in "nihao".chars() {
            let r = p
                .apply(
                    &comp,
                    &KeyEvent {
                        key: Key::Character(c),
                        state: Default::default(),
                    },
                )
                .unwrap();
            comp = r.composition;
            if comp == "nihao" {
                assert!(
                    r.candidates.len() >= 3,
                    "expected at least 3 candidates for nihao, got {:?}",
                    r.candidates.iter().map(|c| &c.text).collect::<Vec<_>>()
                );
                assert_eq!(r.candidates[0].text, "你好");
                assert!(
                    r.candidates.iter().any(|c| c.is_emoji),
                    "should have emoji candidate"
                );
            }
        }
    }
    #[test]
    fn double_pinyin_input_builds_native_segmentor() {
        let pipeline = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: flypy\n  translators:\n    - type: dict\n      dictionary: main\n",
            ),
            None,
            Some(tiny_index()),
        )
        .unwrap();
        let mut composition = String::new();
        let mut update = None;
        for ch in ['v', 's', 'g', 'o'] {
            update = Some(pipeline.apply(&composition, &key(ch)).unwrap());
            composition = update.as_ref().unwrap().composition.clone();
        }
        let update = update.unwrap();
        let china = update
            .candidates
            .iter()
            .find(|candidate| candidate.display.text == "中国")
            .expect("vsgo must produce 中国");
        assert_eq!(china.consumed, InputSpan::new(0, 4));
        assert_eq!(china.canonical_code, "zhong guo");
        assert!(china.complete);
    }

    #[test]
    fn legacy_config_without_input_still_builds_quanpin() {
        let pipeline = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  translators:\n    - type: dict\n      dictionary: main\n",
            ),
            None,
            Some(tiny_index()),
        )
        .unwrap();
        let mut composition = String::new();
        let mut update = None;
        for ch in ['z', 'h', 'o', 'n', 'g'] {
            update = Some(pipeline.apply(&composition, &key(ch)).unwrap());
            composition = update.as_ref().unwrap().composition.clone();
        }
        assert!(
            update
                .unwrap()
                .candidates
                .iter()
                .any(|candidate| candidate.display.text == "中"),
            "legacy quanpin path must keep working"
        );
    }

    #[test]
    fn quanpin_input_with_spelling_correction_works() {
        let pipeline = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  input:\n    type: quanpin\n    spelling_correction:\n      enabled: true\n      max_candidates_per_start: 64\n  translators:\n    - type: dict\n      dictionary: main\n",
            ),
            None,
            Some(tiny_index()),
        )
        .unwrap();
        let mut composition = String::new();
        let mut update = None;
        for ch in ['z', 'h', 'o', 'g', 'n'] {
            update = Some(pipeline.apply(&composition, &key(ch)).unwrap());
            composition = update.as_ref().unwrap().composition.clone();
        }
        assert!(
            update
                .unwrap()
                .candidates
                .iter()
                .any(|candidate| candidate.display.text == "中"),
            "spelling correction must recover 中 from zhogn"
        );
    }

    #[test]
    fn double_pinyin_custom_scheme_keys_build() {
        let index = Arc::new(CompiledIndex::build(
            vec![DictEntry {
                text: "啊".into(),
                code: "a".into(),
                weight: Some(10),
                stem: None,
            }],
            DeploymentGeneration::new(1),
        ));
        let pipeline = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      keys:\n        - key: a\n          finals: [a]\n          single: true\n        - key: b\n          initial: b\n          finals: [a]\n  translators:\n    - type: dict\n      dictionary: main\n",
            ),
            None,
            Some(index),
        )
        .unwrap();
        let mut composition = String::new();
        let mut update = None;
        for ch in ['a', 'b'] {
            update = Some(pipeline.apply(&composition, &key(ch)).unwrap());
            composition = update.as_ref().unwrap().composition.clone();
        }
        assert_eq!(composition, "ab");
        assert!(
            update
                .unwrap()
                .candidates
                .iter()
                .any(|candidate| candidate.display.text == "啊"),
            "custom scheme ab must decode to the a-syllable candidate"
        );
    }

    #[test]
    fn from_scheme_config_rejects_conflicting_and_malformed_schemes() {
        use crate::double_pinyin::CompiledDoublePinyinTable;
        use cheime_config::schema::{
            DoublePinyinKeyConfig, DoublePinyinPreset, DoublePinyinSchemeConfig,
        };

        // preset + keys are mutually exclusive
        let conflict = DoublePinyinSchemeConfig {
            preset: Some(DoublePinyinPreset::Flypy),
            keys: vec![DoublePinyinKeyConfig {
                key: "a".into(),
                initial: None,
                finals: vec!["a".into()],
                single: true,
            }],
        };
        assert!(CompiledDoublePinyinTable::from_scheme_config(&conflict).is_err());

        // multi-char key must be rejected before the chars().next() expect
        let multi_char = DoublePinyinSchemeConfig {
            preset: None,
            keys: vec![DoublePinyinKeyConfig {
                key: "zh".into(),
                initial: None,
                finals: vec!["a".into()],
                single: true,
            }],
        };
        assert!(CompiledDoublePinyinTable::from_scheme_config(&multi_char).is_err());
    }

    #[test]
    fn double_pinyin_scheme_without_preset_or_keys_is_rejected() {
        let result = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme: {}\n",
            ),
            None,
            None,
        );
        assert!(matches!(
            result,
            Err(BuildError::InvalidDoublePinyinScheme { .. })
        ));
    }
    #[test]
    fn double_pinyin_unknown_keyboard_layout_is_rejected() {
        let error = PipelineFactory::build(
            &conf(
                "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: flypy\n    keyboard_mistouch:\n      enabled: true\n      layout: dvorak\n",
            ),
            None,
            None,
        )
        // `.err()` instead of `.unwrap_err()`: ComposablePipeline is not Debug.
        .err()
        .expect("unknown keyboard layout must fail the build");
        assert!(matches!(
            error,
            BuildError::UnsupportedKeyboardLayout { .. }
        ));
    }
}
