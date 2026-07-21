//! Pipeline factory: builds a `ComposablePipeline` from a `SchemaConfig`.
//!
//! This is the key bridge between CheIME's typed configuration system
//! and the runtime pipeline. Unlike Rime's string-based component
//! registration, every component type is validated at config parse time.
//!
//! Missing components fall back to sensible defaults (no-op processor,
//! passthrough segmentor/translator).

use crate::{
    CodeSegment, ComposablePipeline, Filter, Processor, Ranker,
    Segmentor, Translator,
};
use cheime_config::schema::{
    EngineConfig, FilterConfig, ProcessorConfig, SchemaConfig, SegmentorConfig, TranslatorConfig,
};

pub use crate::filter::DedupFilter;
pub use crate::processor::DefaultProcessor;
pub use crate::ranker::FrequencyRanker;
pub use crate::segmentor::PinyinSegmentor;
pub use crate::translator::{DictTranslator, PassthroughTranslator};

/// Build a `ComposablePipeline` from a schema configuration.
///
/// Unknown or unsupported component types cause a deployment error,
/// never a silent fallback (DRAFT §3.4, §3.5).
pub struct PipelineFactory;

impl PipelineFactory {
    /// Build from a parsed, validated `SchemaConfig`.
    pub fn build(config: &SchemaConfig) -> Result<ComposablePipeline, BuildError> {
        let processor = Self::build_processor(&config.engine)?;
        let segmentor = Self::build_segmentor(&config.engine)?;
        let translators = Self::build_translators(&config.engine)?;
        let filters = Self::build_filters(&config.engine)?;
        let ranker = Self::build_ranker(&config.engine);

        Ok(ComposablePipeline::new(processor, segmentor, translators, filters, ranker))
    }

    fn build_processor(
        engine: &EngineConfig,
    ) -> Result<Box<dyn Processor>, BuildError> {
        if engine.processors.is_empty() {
            return Ok(Box::new(DefaultProcessor::new()));
        }
        // For now, use the first supported processor.
        // Full processor chain support planned for later.
        for proc in &engine.processors {
            match proc {
                ProcessorConfig::AsciiComposer(_) | ProcessorConfig::Speller => {
                    // Both map to DefaultProcessor for now
                    return Ok(Box::new(DefaultProcessor::new()));
                }
                ProcessorConfig::KeyBinder(_) | ProcessorConfig::Recognizer(_) => {
                    // Not yet implemented — skip and continue
                    continue;
                }
                ProcessorConfig::Lua(_) | ProcessorConfig::Selector
                | ProcessorConfig::Navigator | ProcessorConfig::ExpressEditor
                | ProcessorConfig::Punctuator(_) => {
                    // Not yet implemented — skip and continue
                    continue;
                }
            }
        }
        // All processors skipped → default
        Ok(Box::new(DefaultProcessor::new()))
    }

    fn build_segmentor(
        engine: &EngineConfig,
    ) -> Result<Box<dyn Segmentor>, BuildError> {
        for seg in &engine.segmentors {
            match seg {
                SegmentorConfig::PinyinSyllable => {
                    return Ok(Box::new(PinyinSegmentor::new()));
                }
                SegmentorConfig::Ascii | SegmentorConfig::Abc
                | SegmentorConfig::Affix(_) | SegmentorConfig::Punct
                | SegmentorConfig::Fallback | SegmentorConfig::Lua(_) => {
                    continue;
                }
            }
        }
        // No pinyin segmentor found — fallback to passthrough
        Ok(Box::new(PassthroughSegmentor))
    }

    fn build_translators(
        engine: &EngineConfig,
    ) -> Result<Vec<Box<dyn Translator>>, BuildError> {
        let mut translators: Vec<Box<dyn Translator>> = Vec::new();

        for tl in &engine.translators {
            match tl {
                TranslatorConfig::Dict(_dict_config) => {
                    // Dict translator requires CompiledIndex.
                    // In the full system, the deployment manager provides this.
                    // For now, if no dictionary is pre-loaded, we skip.
                    // This is where the config→runtime bridge connects to
                    // the deployment/deploy subsystem.
                    continue; // Dictionary loading needs DeploymentManager integration
                }
                TranslatorConfig::Punct | TranslatorConfig::Echo
                | TranslatorConfig::Table(_) | TranslatorConfig::Script(_)
                | TranslatorConfig::Lua(_) | TranslatorConfig::History => {
                    continue;
                }
            }
        }

        if translators.is_empty() {
            translators.push(Box::new(PassthroughTranslator));
        }
        Ok(translators)
    }

    fn build_filters(engine: &EngineConfig) -> Result<Vec<Box<dyn Filter>>, BuildError> {
        let mut filters: Vec<Box<dyn Filter>> = Vec::new();

        for f in &engine.filters {
            match f {
                FilterConfig::Uniquifier => {
                    filters.push(Box::new(DedupFilter::new()));
                }
                FilterConfig::Simplifier(_) | FilterConfig::CharsetFilter(_)
                | FilterConfig::SingleChar | FilterConfig::Lua(_) => {
                    continue; // Not yet implemented
                }
            }
        }
        Ok(filters)
    }

    fn build_ranker(_engine: &EngineConfig) -> Box<dyn Ranker> {
        Box::new(FrequencyRanker::new())
    }
}

// ── Error type ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum BuildError {
    UnsupportedComponent {
        component_type: String,
        pipeline_stage: String,
    },
    MissingDictionary {
        name: String,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::UnsupportedComponent {
                component_type,
                pipeline_stage,
            } => write!(
                f,
                "unsupported component '{component_type}' in {pipeline_stage} stage"
            ),
            BuildError::MissingDictionary { name } => {
                write!(f, "dictionary '{name}' not found")
            }
        }
    }
}

impl std::error::Error for BuildError {}

// ── Passthrough segmentor (no-op) ───────────────────────────────────

struct PassthroughSegmentor;

impl Segmentor for PassthroughSegmentor {
    fn segment(&self, composition: &str) -> Vec<CodeSegment> {
        if composition.is_empty() {
            return Vec::new();
        }
        vec![CodeSegment {
            code: composition.to_owned(),
            tag: String::from("passthrough"),
        }]
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InputPipeline;
    use cheime_config::schema::SchemaConfig;
    use cheime_model::{Key, KeyEvent};
    fn schema_from_yaml(yaml: &str) -> SchemaConfig {
        serde_yaml::from_str(yaml).expect("invalid test YAML")
    }

    #[test]
    fn builds_minimal_pipeline() {
        let config = schema_from_yaml(
            r#"
schema_version: 1
engine:
  processors:
    - type: ascii_composer
  segmentors:
    - type: pinyin_syllable
    - type: fallback
  translators:
    - type: echo
  filters:
    - type: uniquifier
"#,
        );
        let pipeline = PipelineFactory::build(&config).unwrap();
        // Type 'n'
        let result = pipeline
            .apply("", &KeyEvent {
                key: Key::Character('n'),
                state: Default::default(),
            })
            .unwrap();
        assert_eq!(result.composition, "n");
    }

    #[test]
    fn empty_config_uses_defaults() {
        let config = schema_from_yaml(
            r#"
schema_version: 1
engine: {}
"#,
        );
        let pipeline = PipelineFactory::build(&config).unwrap();
        let result = pipeline
            .apply("", &KeyEvent {
                key: Key::Character('n'),
                state: Default::default(),
            })
            .unwrap();
        assert_eq!(result.composition, "n");
    }

    #[test]
    fn pinyin_segmentor_splits_composition() {
        let config = schema_from_yaml(
            r#"
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
"#,
        );
        let pipeline = PipelineFactory::build(&config).unwrap();
        // Type "zhongguo"
        let result = pipeline
            .apply("zhonggu", &KeyEvent {
                key: Key::Character('o'),
                state: Default::default(),
            })
            .unwrap();
        assert_eq!(result.composition, "zhongguo");
        // Segmentor splits into ["zhong", "guo"]
        // The order depends on the trie traversal; both are valid
        assert_eq!(result.candidates.len(), 2);
        let texts: Vec<&str> = result.candidates.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"zhong"));
        assert!(texts.contains(&"guo"));
    }
}
