//! Typed configuration schema for an input method.
//!
//! Every field maps to a Rust type. Serde's `deny_unknown_fields` ensures
//! that typos and unsupported options are caught at parse time.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── Schema-level config ─────────────────────────────────────────────

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaMeta>,

    #[serde(default)]
    pub engine: EngineConfig,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub switches: Vec<SwitchGroup>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speller: Option<SpellerConfig>,

    #[serde(default)]
    pub menu: MenuConfig,

    #[serde(default = "default_version")]
    pub schema_version: u32,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punctuator: Option<PunctuatorConfig>,
}

fn default_version() -> u32 {
    1
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Engine pipeline ─────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub processors: Vec<ProcessorConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segmentors: Vec<SegmentorConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translators: Vec<TranslatorConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuzzy_pinyin: Option<FuzzyPinyinConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FuzzyPinyinConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Specific rules to enable (e.g. ["zh_z", "n_l"]). Empty = all standard rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
}

// ── Processor configs ───────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ProcessorConfig {
    #[serde(rename = "ascii_composer")]
    AsciiComposer(AsciiComposerConfig),

    #[serde(rename = "recognizer")]
    Recognizer(RecognizerConfig),

    #[serde(rename = "key_binder")]
    KeyBinder(KeyBinderConfig),

    #[serde(rename = "speller")]
    Speller,

    #[serde(rename = "punctuator")]
    Punctuator(PunctuatorConfig),

    #[serde(rename = "selector")]
    Selector,

    #[serde(rename = "navigator")]
    Navigator,

    #[serde(rename = "express_editor")]
    ExpressEditor,

    #[serde(rename = "lua")]
    Lua(LuaComponentRef),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AsciiComposerConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub switch_key: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecognizerConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub patterns: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBinderConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<KeyBinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    pub when: String,
    pub accept: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PunctuatorConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub full_shape: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub half_shape: BTreeMap<String, serde_json::Value>,
}

// ── Segmentor configs ───────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum SegmentorConfig {
    #[serde(rename = "pinyin_syllable")]
    PinyinSyllable,

    #[serde(rename = "ascii")]
    Ascii,

    #[serde(rename = "abc")]
    Abc,

    #[serde(rename = "affix")]
    Affix(AffixSegmentorConfig),

    #[serde(rename = "punct")]
    Punct,

    #[serde(rename = "fallback")]
    Fallback,

    #[serde(rename = "lua")]
    Lua(LuaComponentRef),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AffixSegmentorConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

// ── Translator configs ──────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum TranslatorConfig {
    #[serde(rename = "dict")]
    Dict(DictTranslatorConfig),

    #[serde(rename = "table")]
    Table(TableTranslatorConfig),

    #[serde(rename = "script")]
    Script(ScriptTranslatorConfig),

    #[serde(rename = "punct")]
    Punct,

    #[serde(rename = "echo")]
    Echo,
    #[serde(rename = "lua")]
    Lua(LuaComponentRef),

    #[serde(rename = "emoji")]
    Emoji(EmojiTranslatorConfig),

    #[serde(rename = "history")]
    History,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DictTranslatorConfig {
    pub dictionary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default = "default_true")]
    pub enable_completion: bool,
    #[serde(default = "default_true")]
    pub enable_sentence: bool,
    #[serde(default)]
    pub initial_quality: f64,
}

impl Default for DictTranslatorConfig {
    fn default() -> Self {
        Self {
            dictionary: String::new(),
            r#ref: None,
            enable_completion: true,
            enable_sentence: true,
            initial_quality: 0.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TableTranslatorConfig {
    pub dictionary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default = "default_true")]
    pub enable_completion: bool,
    #[serde(default = "default_true")]
    pub enable_sentence: bool,
    #[serde(default)]
    pub initial_quality: f64,
}

impl Default for TableTranslatorConfig {
    fn default() -> Self {
        Self {
            dictionary: String::new(),
            r#ref: None,
            enable_completion: true,
            enable_sentence: true,
            initial_quality: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptTranslatorConfig {
    pub dictionary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default = "default_true")]
    pub enable_completion: bool,
    #[serde(default = "default_true")]
    pub enable_sentence: bool,
    #[serde(default)]
    pub initial_quality: f64,
    #[serde(default)]
    pub enable_correction: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prism: Option<String>,
}

/// Emoji translator: loads emoji data from an external TSV file.
///
/// File format: `emoji<TAB>keywords(space-sep)<TAB>pinyin(space-sep)`
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmojiTranslatorConfig {
    /// Path to emoji data file (relative to config dir, or absolute).
    /// Default: "data/emoji.txt"
    #[serde(default = "default_emoji_data")]
    pub emoji_data: String,
}

fn default_emoji_data() -> String {
    String::from("data/emoji.txt")
}

// ── Filter configs ──────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum FilterConfig {
    #[serde(rename = "uniquifier")]
    Uniquifier,

    #[serde(rename = "simplifier")]
    Simplifier(SimplifierConfig),

    #[serde(rename = "charset_filter")]
    CharsetFilter(CharsetFilterConfig),

    #[serde(rename = "single_char")]
    SingleChar,

    #[serde(rename = "lua")]
    Lua(LuaComponentRef),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimplifierConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencc_config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tips: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CharsetFilterConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
}

// ── Shared types ────────────────────────────────────────────────────

/// Reference to a Lua component.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LuaComponentRef {
    pub r#ref: String,
}

// ── Switches ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub switches: Vec<SwitchConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchConfig {
    pub id: String,
    pub label: String,
    pub states: Vec<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<SwitchDependency>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchDependency {
    pub switch: String,
    pub state: u8,
}

// ── Speller ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpellerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alphabet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default = "default_max_code")]
    pub max_code_length: usize,
    #[serde(default)]
    pub auto_select: bool,
    #[serde(default)]
    pub use_space: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub algebra: Vec<SpellerAlgebra>,
}

fn default_max_code() -> usize {
    0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpellerAlgebra {
    pub rule: String,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
}

// ── Menu ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MenuConfig {
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    #[serde(default)]
    pub page_down_cycle: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternative_select_keys: Option<String>,
}

impl Default for MenuConfig {
    fn default() -> Self {
        Self {
            page_size: default_page_size(),
            page_down_cycle: false,
            alternative_select_keys: None,
        }
    }
}

fn default_page_size() -> usize {
    9
}

fn default_true() -> bool {
    true
}
// ── Tests ───────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_schema() {
        let yaml = r#"
schema_version: 1
engine:
  processors:
    - type: ascii_composer
    - type: speller
  segmentors:
    - type: pinyin_syllable
    - type: fallback
  translators:
    - type: dict
      dictionary: luna_pinyin
  filters:
    - type: uniquifier
menu:
  page_size: 9
"#;
        let config: SchemaConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.engine.processors.len(), 2);
        assert_eq!(config.engine.segmentors.len(), 2);
        assert_eq!(config.engine.translators.len(), 1);
        let TranslatorConfig::Dict(dict) = &config.engine.translators[0] else {
            panic!("expected dictionary translator");
        };
        assert!(dict.enable_completion);
        assert!(dict.enable_sentence);
        assert_eq!(config.menu.page_size, 9);
    }

    #[test]
    fn parse_full_schema_with_all_component_types() {
        let yaml = r#"
schema_version: 1
schema:
  schema_id: test_schema
  name: 测试方案
engine:
  processors:
    - type: ascii_composer
      switch_key:
        Caps_Lock: clear
    - type: recognizer
      patterns:
        email: "^[a-z]+@.*$"
    - type: key_binder
      bindings:
        - when: composing
          accept: Tab
          send: Shift+Right
    - type: speller
    - type: selector
    - type: navigator
  segmentors:
    - type: pinyin_syllable
    - type: affix
      tag: reverse_lookup
      prefix: "`"
  translators:
    - type: dict
      dictionary: luna_pinyin
      ref: main_dict
      enable_completion: true
      initial_quality: 1.2
    - type: lua
      ref: date_translator
  filters:
    - type: uniquifier
    - type: simplifier
      opencc_config: s2t.json
switches:
  - group: 输入模式
    switches:
      - id: ascii_mode
        label: 中/英
        states: ["中", "Ａ"]
speller:
  alphabet: "abcdefghijklmnopqrstuvwxyz"
  algebra:
    - rule: fuzz
      pattern: "zh"
      to: "z"
menu:
  page_size: 9
"#;
        let config: SchemaConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.schema.as_ref().unwrap().schema_id.as_deref(),
            Some("test_schema")
        );
        assert_eq!(config.engine.processors.len(), 6);
        assert_eq!(config.engine.translators.len(), 2);

        match &config.engine.translators[0] {
            TranslatorConfig::Dict(d) => {
                assert_eq!(d.dictionary, "luna_pinyin");
                assert_eq!(d.r#ref.as_deref(), Some("main_dict"));
                assert!((d.initial_quality - 1.2).abs() < 0.001);
            }
            other => panic!("expected Dict, got {other:?}"),
        }

        assert_eq!(config.switches.len(), 1);
        assert_eq!(config.speller.as_ref().unwrap().algebra.len(), 1);
    }

    #[test]
    fn unknown_field_is_error() {
        let yaml = r#"
schema_version: 1
engine:
  processors:
    - type: ascii_composer
      nonexistent_field: "this should fail"
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn extends_chain_is_preserved() {
        let yaml = r#"
schema_version: 1
extends:
  - base_pinyin
  - shared/common
engine: {}
"#;
        let config: SchemaConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.extends, vec!["base_pinyin", "shared/common"]);
    }

    #[test]
    fn unknown_field_in_engine_is_rejected() {
        let yaml = r#"
schema_version: 1
engine:
  processors: []
  unknown_engine_field: "should fail"
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_field_in_filter_is_rejected() {
        let yaml = r#"
schema_version: 1
engine:
  processors: []
  filters:
    - type: simplifier
      opencc_config: s2t.json
      bogus_option: true
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_processor_type_is_rejected() {
        let yaml = r#"
schema_version: 1
engine:
  processors:
    - type: imaginary_processor
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_segmentor_type_is_rejected() {
        let yaml = r#"
schema_version: 1
engine:
  segmentors:
    - type: imaginary_segmentor
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_translator_type_is_rejected() {
        let yaml = r#"
schema_version: 1
engine:
  translators:
    - type: imaginary_translator
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
