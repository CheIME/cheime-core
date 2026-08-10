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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinyin_correction: Option<PinyinCorrectionConfig>,

    /// Input scheme. `None` = legacy: `segmentors` + `pinyin_correction`
    /// drive the pipeline (quanpin semantics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputConfig>,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinyinCorrectionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_max_edit_distance")]
    pub max_edit_distance: u8,
    #[serde(default = "default_max_correction_candidates")]
    pub max_candidates_per_start: usize,
    #[serde(default = "default_edit_penalty")]
    pub edit_penalty: i64,
}

impl Default for PinyinCorrectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_edit_distance: default_max_edit_distance(),
            max_candidates_per_start: default_max_correction_candidates(),
            edit_penalty: default_edit_penalty(),
        }
    }
}

// ── Input scheme configs ────────────────────────────────────────────

/// Input scheme selector. `QuanPin` and `DoublePinyin` differ only in the
/// raw-code → canonical-syllable stage; everything downstream is shared.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputConfig {
    #[serde(rename = "quanpin")]
    QuanPin(QuanPinInputConfig),
    #[serde(rename = "double_pinyin")]
    DoublePinyin(DoublePinyinInputConfig),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuanPinInputConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spelling_correction: Option<PinyinCorrectionConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoublePinyinInputConfig {
    pub scheme: DoublePinyinSchemeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_mistouch: Option<KeyboardMistouchConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_confusion: Option<CodeConfusionConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoublePinyinSchemeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<DoublePinyinPreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<DoublePinyinKeyConfig>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DoublePinyinPreset {
    Flypy,
    MsDouble,
    Ziranma,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoublePinyinKeyConfig {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<String>,
    pub finals: Vec<String>,
    #[serde(default)]
    pub single: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyboardMistouchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_keyboard_mistouch_cost")]
    pub cost: i64,
    #[serde(default = "default_keyboard_layout")]
    pub layout: String,
}

impl Default for KeyboardMistouchConfig {
    /// Must mirror the serde defaults — a consumer building this via
    /// `Default::default()` must get the load-bearing contract values.
    fn default() -> Self {
        Self {
            enabled: false,
            cost: default_keyboard_mistouch_cost(),
            layout: default_keyboard_layout(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodeConfusionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_code_confusion_cost")]
    pub cost: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<CodeConfusionRuleConfig>,
}

impl Default for CodeConfusionConfig {
    /// Must mirror the serde defaults — see [`KeyboardMistouchConfig::default`].
    fn default() -> Self {
        Self {
            enabled: false,
            cost: default_code_confusion_cost(),
            rules: Vec::new(),
        }
    }
}

/// Directional confusion rule: the user typed `from` but meant `to`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodeConfusionRuleConfig {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<i64>,
}

fn default_keyboard_mistouch_cost() -> i64 {
    350_000
}

fn default_keyboard_layout() -> String {
    String::from("qwerty")
}

fn default_code_confusion_cost() -> i64 {
    250_000
}

fn default_max_edit_distance() -> u8 {
    1
}

fn default_max_correction_candidates() -> usize {
    16
}

fn default_edit_penalty() -> i64 {
    500_000
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
    fn parse_bounded_pinyin_correction_config() {
        let yaml = r#"
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
  pinyin_correction:
    enabled: true
    max_edit_distance: 2
    max_candidates_per_start: 12
    edit_penalty: 9000
"#;
        let config: SchemaConfig = serde_yaml::from_str(yaml).unwrap();
        let correction = config.engine.pinyin_correction.expect("correction config");

        assert!(correction.enabled);
        assert_eq!(correction.max_edit_distance, 2);
        assert_eq!(correction.max_candidates_per_start, 12);
        assert_eq!(correction.edit_penalty, 9_000);
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
    fn parse_schema(yaml: &str) -> SchemaConfig {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn input_double_pinyin_preset_parses() {
        let config = parse_schema(
            "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: flypy\n",
        );
        let Some(InputConfig::DoublePinyin(input)) = &config.engine.input else {
            panic!("expected double_pinyin input");
        };
        assert_eq!(input.scheme.preset, Some(DoublePinyinPreset::Flypy));
        assert!(input.scheme.keys.is_empty());
        assert!(input.keyboard_mistouch.is_none());
        assert!(input.code_confusion.is_none());
    }

    #[test]
    fn input_double_pinyin_full_config_parses() {
        let config = parse_schema(
            "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: ziranma\n    keyboard_mistouch:\n      enabled: true\n      cost: 350000\n      layout: qwerty\n    code_confusion:\n      enabled: true\n      cost: 250000\n      rules:\n        - from: vd\n          to: vs\n        - from: ab\n          to: ad\n          cost: 180000\n",
        );
        let Some(InputConfig::DoublePinyin(input)) = &config.engine.input else {
            panic!("expected double_pinyin input");
        };
        assert_eq!(input.scheme.preset, Some(DoublePinyinPreset::Ziranma));
        let mistouch = input.keyboard_mistouch.as_ref().unwrap();
        assert!(mistouch.enabled);
        assert_eq!(mistouch.cost, 350_000);
        assert_eq!(mistouch.layout, "qwerty");
        let confusion = input.code_confusion.as_ref().unwrap();
        assert!(confusion.enabled);
        assert_eq!(confusion.cost, 250_000);
        assert_eq!(confusion.rules.len(), 2);
        assert_eq!(confusion.rules[0].from, "vd");
        assert_eq!(confusion.rules[0].to, "vs");
        assert_eq!(confusion.rules[0].cost, None);
        assert_eq!(confusion.rules[1].cost, Some(180_000));
    }

    #[test]
    fn input_double_pinyin_defaults_apply_when_omitted() {
        let config = parse_schema(
            "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: flypy\n    keyboard_mistouch:\n      enabled: true\n    code_confusion:\n      enabled: true\n",
        );
        let Some(InputConfig::DoublePinyin(input)) = &config.engine.input else {
            panic!("expected double_pinyin input");
        };
        let mistouch = input.keyboard_mistouch.as_ref().unwrap();
        assert!(mistouch.enabled);
        assert_eq!(
            mistouch.cost, 350_000,
            "omitted cost must fall back to the contract default"
        );
        assert_eq!(mistouch.layout, "qwerty");
        let confusion = input.code_confusion.as_ref().unwrap();
        assert!(confusion.enabled);
        assert_eq!(confusion.cost, 250_000);
        // Rust Default impls must mirror the serde defaults.
        assert_eq!(KeyboardMistouchConfig::default().cost, 350_000);
        assert_eq!(KeyboardMistouchConfig::default().layout, "qwerty");
        assert_eq!(CodeConfusionConfig::default().cost, 250_000);
    }

    #[test]
    fn input_double_pinyin_custom_keys_parse() {
        let config = parse_schema(
            "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      keys:\n        - key: a\n          finals: [a]\n          single: true\n        - key: b\n          initial: b\n          finals: [a]\n",
        );
        let Some(InputConfig::DoublePinyin(input)) = &config.engine.input else {
            panic!("expected double_pinyin input");
        };
        assert!(input.scheme.preset.is_none());
        assert_eq!(input.scheme.keys.len(), 2);
        assert_eq!(input.scheme.keys[0].key, "a");
        assert_eq!(input.scheme.keys[0].finals, vec![String::from("a")]);
        assert!(input.scheme.keys[0].single);
        assert_eq!(input.scheme.keys[1].initial.as_deref(), Some("b"));
    }

    #[test]
    fn input_quanpin_parses() {
        let config = parse_schema(
            "schema_version: 1\nengine:\n  input:\n    type: quanpin\n    spelling_correction:\n      enabled: true\n      max_edit_distance: 2\n",
        );
        let Some(InputConfig::QuanPin(input)) = &config.engine.input else {
            panic!("expected quanpin input");
        };
        let correction = input.spelling_correction.as_ref().unwrap();
        assert!(correction.enabled);
        assert_eq!(correction.max_edit_distance, 2);
    }

    #[test]
    fn input_absent_round_trips_to_none() {
        let config = parse_schema("schema_version: 1\nengine: {}\n");
        assert!(config.engine.input.is_none());
    }

    #[test]
    fn input_rejects_unknown_variant() {
        assert!(
            serde_yaml::from_str::<SchemaConfig>(
                "schema_version: 1\nengine:\n  input:\n    type: wubi\n"
            )
            .is_err()
        );
    }

    #[test]
    fn input_rejects_unknown_field() {
        assert!(serde_yaml::from_str::<SchemaConfig>(
            "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: flypy\n    extra: 1\n"
        )
        .is_err());
    }

    #[test]
    fn input_rejects_unknown_preset() {
        assert!(serde_yaml::from_str::<SchemaConfig>(
            "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: bogus\n"
        )
        .is_err());
    }
}
