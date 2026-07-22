//! Integration / stress tests for ComposablePipeline with the real rime_ice dict (539K entries).
//!
//! Verifies correctness and catches regressions at scale.

use cheime_config::schema::SchemaConfig;
use cheime_dictionary::{parse_body, CompiledIndex, DictColumn};
use cheime_model::{DeploymentGeneration, Key, KeyEvent, KeyState};
use cheime_pipeline::factory::PipelineFactory;
use cheime_pipeline::InputPipeline;
use std::sync::{Arc, OnceLock};
use cheime_config::schema::PunctuatorConfig;
use cheime_pipeline::normalizer::FuzzyNormalizer;
use cheime_pipeline::processor::DefaultProcessor;
use cheime_pipeline::segmentor::PinyinSegmentor;
use cheime_pipeline::simplifier::{Conversion, SimplifierFilter};
use cheime_pipeline::translator::DictTranslator;
use cheime_pipeline::ranker::UnifiedRanker;
use cheime_pipeline::ComposablePipeline;
use std::collections::BTreeMap;

// ── Shared real-dict pipeline ──────────────────────────────────────

fn real_dict() -> &'static Arc<CompiledIndex> {
    static DICT: OnceLock<Arc<CompiledIndex>> = OnceLock::new();
    DICT.get_or_init(|| {
        let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
        let body = dict_body(raw);
        let cols = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];
        let entries = parse_body(body, cols).unwrap();
        Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1)))
    })
}

fn dict_body(raw: &str) -> &str {
    if let Some(p) = raw.find("\n...\n") { &raw[p + 5..] } else { raw }
}

fn real_pipeline() -> impl InputPipeline {
    PipelineFactory::build(
        &serde_yaml::from_str::<SchemaConfig>(
            "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n"
        ).unwrap(),
        None,
        Some(real_dict().clone()),
        None,
    ).unwrap()
}

fn key(ch: char) -> KeyEvent { KeyEvent { key: Key::Character(ch), state: KeyState::default() } }
fn backspace() -> KeyEvent { KeyEvent { key: Key::Backspace, state: KeyState::default() } }

// ── Typing simulation ──────────────────────────────────────────────

#[test]
fn typing_zhongguo_produces_candidates() {
    let p = real_pipeline();
    let steps = "zhongguo";
    let mut comp = String::new();
    let mut found = false;
    for ch in steps.chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
        if update.candidates.iter().any(|c| c.text == "中国") {
            found = true;
        }
    }
    assert!(found, "should find 中国 after typing zhongguo");
}

#[test]
fn typing_long_phrase_produces_candidates() {
    let p = real_pipeline();
    // Type zhonghuarenmin — a long pinyin phrase
    let steps = "zhonghuarenmin";
    let mut comp = String::new();
    let mut total: usize = 0;
    for ch in steps.chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
        total = total.wrapping_add(update.candidates.len());
    }
    assert!(total > 0, "should produce candidates during typing");
}

#[test]
fn typing_nihao_ranked_correctly() {
    let p = real_pipeline();
    let mut comp = String::new();
    for ch in "nihao".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
        if comp == "nihao" {
            let texts: Vec<&str> = update.candidates.iter().map(|c| c.text.as_str()).collect();
            assert!(texts.contains(&"你好"), "nihao should have 你好, got {:?}", texts);
            assert!(texts.contains(&"拟好"), "nihao should have 拟好");
        }
    }
}

// ── Backspace + re-type ────────────────────────────────────────────

#[test]
fn backspace_and_retype() {
    let p = real_pipeline();
    let mut comp = String::new();

    for ch in "niha".chars() {
        comp = p.apply(&comp, &key(ch)).unwrap().composition;
    }
    assert_eq!(comp, "niha");

    // Backspace removes one char
    comp = p.apply(&comp, &backspace()).unwrap().composition;
    assert_eq!(comp, "nih");

    // Re-type "ao" → "nihao"
    for ch in "ao".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
        if comp == "nihao" {
            assert!(!update.candidates.is_empty());
        }
    }
}

// ── Empty input → enter ────────────────────────────────────────────

#[test]
fn empty_composition_has_no_candidates() {
    let p = real_pipeline();
    let result = p.apply("", &KeyEvent { key: Key::Enter, state: KeyState::default() }).unwrap();
    assert!(result.candidates.is_empty());
}

// ── Rapid keystroke stress ─────────────────────────────────────────

#[test]
fn rapid_keystrokes_and_full_backspace() {
    let p = real_pipeline();
    let phrase = "zhonghuashanghaibeijingtianjin";
    let mut comp = String::new();
    for ch in phrase.chars() {
        if ch.is_ascii_lowercase() {
            comp = p.apply(&comp, &key(ch)).unwrap().composition;
        }
    }
    while !comp.is_empty() {
        comp = p.apply(&comp, &backspace()).unwrap().composition;
    }
    assert!(comp.is_empty());
}

// ── Emoji in results ───────────────────────────────────────────────

#[test]
fn emoji_appears_in_pinyin_search() {
    let p = real_pipeline();
    let mut comp = String::new();
    for ch in "nihao".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
    }
    let has_emoji = comp.chars().count() >= 5 && p.apply(&comp[..4], &key('o')).unwrap().candidates.iter().any(|c| c.is_emoji);
    assert!(has_emoji, "nihao should produce emoji candidate 👋");
}

// ── Punctuator integration ──────────────────────────────────────────

#[test]
fn punctuator_dot_commits_fullwidth_period() {
    let config_yaml = r#"
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
punctuator:
  full_shape:
    ".": {commit: "。"}
  half_shape: {}
"#;
    let config: SchemaConfig = serde_yaml::from_str(config_yaml).unwrap();
    let p = cheime_pipeline::factory::PipelineFactory::build(
        &config, None, Some(real_dict().clone()), None,
    ).unwrap();

    let result = p.apply("", &KeyEvent { key: Key::Character('.'), state: KeyState::default() }).unwrap();
    // With commit intent, the composition should be unchanged and CommitText("。") should fire
    assert!(matches!(result.intent, cheime_pipeline::PipelineIntent::CommitText(ref t) if t == "。"),
        "expected CommitText(。), got {:?}", result.intent);
}

#[test]
fn punctuator_pipe_shows_candidates() {
    let config_yaml = r#"
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
punctuator:
  full_shape:
    "|": ["·", "｜", "§", "¦"]
  half_shape: {}
"#;
    let config: SchemaConfig = serde_yaml::from_str(config_yaml).unwrap();
    let p = cheime_pipeline::factory::PipelineFactory::build(
        &config, None, Some(real_dict().clone()), None,
    ).unwrap();

    let result = p.apply("", &KeyEvent { key: Key::Character('|'), state: KeyState::default() }).unwrap();
    assert_eq!(result.composition, "|");
    assert_eq!(result.candidates.len(), 4, "expected 4 candidates, got {:?}", result.candidates.iter().map(|c| &c.text).collect::<Vec<_>>());
    assert!(result.candidates.iter().any(|c| c.text == "·"));
    assert!(result.candidates.iter().any(|c| c.text == "｜"));
    assert!(result.candidates.iter().any(|c| c.text == "§"));
    assert!(result.candidates.iter().any(|c| c.text == "¦"));
}

#[test]
fn punctuator_after_digit_dot_stays_halfwidth() {
    let config_yaml = r#"
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
punctuator:
  full_shape:
    ".": {commit: "。"}
  half_shape: {}
"#;
    let config: SchemaConfig = serde_yaml::from_str(config_yaml).unwrap();
    let p = cheime_pipeline::factory::PipelineFactory::build(
        &config, None, Some(real_dict().clone()), None,
    ).unwrap();

    // Type '3' then '.'
    let r1 = p.apply("", &KeyEvent { key: Key::Character('3'), state: KeyState::default() }).unwrap();
    assert_eq!(r1.composition, "3");

    let r2 = p.apply("3", &KeyEvent { key: Key::Character('.'), state: KeyState::default() }).unwrap();
    // Should append '.' as half-width, not commit '。'
    assert_eq!(r2.composition, "3.");
    assert!(!matches!(r2.intent, cheime_pipeline::PipelineIntent::CommitText(_)),
        "should not commit after digit, got {:?}", r2.intent);
}

#[test]
fn punctuator_digit_tracking_resets_on_non_digit() {
    let config_yaml = r#"
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
punctuator:
  full_shape:
    ".": {commit: "。"}
  half_shape: {}
"#;
    let config: SchemaConfig = serde_yaml::from_str(config_yaml).unwrap();
    let p = cheime_pipeline::factory::PipelineFactory::build(
        &config, None, Some(real_dict().clone()), None,
    ).unwrap();

    // "3n." — the 'n' resets digit tracking, so '.' should commit '。'
    p.apply("", &KeyEvent { key: Key::Character('3'), state: KeyState::default() }).unwrap();
    p.apply("3", &KeyEvent { key: Key::Character('n'), state: KeyState::default() }).unwrap();
    let r3 = p.apply("3n", &KeyEvent { key: Key::Character('.'), state: KeyState::default() }).unwrap();
    assert!(matches!(r3.intent, cheime_pipeline::PipelineIntent::CommitText(ref t) if t == "。"),
        "after letter, . should commit fullwidth, got {:?}", r3.intent);
}

// ── Fuzzy normalizer integration ────────────────────────────────────

#[test]
fn fuzzy_pinyin_matches_with_normalizer() {
    let p = ComposablePipeline::new(
        Box::new(DefaultProcessor::new()),
        Box::new(PinyinSegmentor::new()),
        Some(Box::new(FuzzyNormalizer::standard())),
        vec![
            Box::new(DictTranslator::new("main", real_dict().clone())),
            Box::new(cheime_pipeline::emoji::EmojiTranslator::empty()),
        ],
        vec![],
        Box::new(UnifiedRanker::new(Default::default())),
    );

    let mut comp = String::new();
    let mut found = false;
    for ch in "zhong".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
        if !update.candidates.is_empty() {
            found = true;
        }
    }
    assert!(found, "should produce candidates for zhong with fuzzy normalizer in pipeline");
}
#[test]
fn punctuator_half_shape_does_not_convert() {
    let mut full = BTreeMap::new();
    full.insert(".".into(), serde_json::json!({"commit": "。"}));
    let half = BTreeMap::new();
    let config = PunctuatorConfig { full_shape: full, half_shape: half };

    let p = ComposablePipeline::new(
        Box::new(cheime_pipeline::punctuator::PunctProcessor::new(
            &config, true, Box::new(DefaultProcessor::new()))),
        Box::new(PinyinSegmentor::new()),
        None,
        vec![Box::new(DictTranslator::new("main", real_dict().clone()))],
        vec![],
        Box::new(UnifiedRanker::new(Default::default())),
    );

    // In half_shape mode with empty map, "." is not intercepted.
    // It falls through to inner DefaultProcessor which rejects
    // non-alphanumeric characters. The key assertion: "." does
    // NOT commit "。" unlike full_shape mode.
    let result = p.apply("", &KeyEvent { key: Key::Character('.'), state: KeyState::default() });
    match result {
        Err(cheime_pipeline::PipelineError::UnsupportedCharacter('.')) => {
            // Correct: half_shape mode passes through without converting
        }
        Ok(_) => {
            panic!("expected UnsupportedCharacter in half_shape mode with no '.' mapping");
        }
        Err(e) => panic!("unexpected error: {:?}", e),
    }
}

// ── Simplifier source annotation ────────────────────────────────────

#[test]
fn simplifier_annotated_source_preserved_in_candidates() {
    // Build a minimal s2t simplifier with annotation enabled
    let tsv = "# minimal s2t\n国\t國\n";
    let sf = SimplifierFilter::parse(tsv, Conversion::S2T, true).unwrap();

    let p = ComposablePipeline::new(
        Box::new(DefaultProcessor::new()),
        Box::new(PinyinSegmentor::new()),
        None,
        vec![Box::new(DictTranslator::new("main", real_dict().clone()))],
        vec![Box::new(sf)],
        Box::new(UnifiedRanker::new(Default::default())),
    );

    let mut comp = String::new();
    let mut final_update = None;
    for ch in "zhongguo".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition.clone();
        final_update = Some(update);
    }
    let update = final_update.expect("should have candidates after typing zhongguo");
    assert!(!update.candidates.is_empty(), "should have candidates for zhongguo");

    // Look for a candidate whose source contains "simplified" (went through conversion)
    let simplified = update.candidates.iter().find(|c| c.source.contains("simplified"));
    assert!(simplified.is_some(),
        "should have candidate with 'simplified' source annotation, got sources: {:?}",
        update.candidates.iter().map(|c| &c.source).collect::<Vec<_>>());

    let c = simplified.unwrap();
    assert!(c.text.contains('國'),
        "simplified candidate should contain traditional character, got '{}'", c.text);
    assert!(c.source.starts_with("dict"),
        "annotated source should start with 'dict', got '{}'", c.source);
}

// ── Fuzzy pinyin via config ─────────────────────────────────────────

#[test]
fn fuzzy_config_zongguo_matches_zhongguo() {
    let config: SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  fuzzy_pinyin:\n    enabled: true\n"
    ).unwrap();
    let p = PipelineFactory::build(&config, None, Some(real_dict().clone()), None).unwrap();

    let mut comp = String::new();
    for ch in "zongguo".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
    }
    let final_update = p.apply(&comp, &key('a')).unwrap();
    assert!(final_update.candidates.iter().any(|c| c.text.starts_with("\u{4e2d}\u{56fd}")),
        "fuzzy z/zh should produce zhongguo candidates, got top5: {:?}",
        final_update.candidates.iter().take(5).map(|c| &c.text).collect::<Vec<_>>());
}

#[test]
fn abbreviation_nh_produces_nihao() {
    let config: SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n"
    ).unwrap();
    let p = PipelineFactory::build(&config, None, Some(real_dict().clone()), None).unwrap();

    let mut comp = String::new();
    let mut last_update = None;
    for ch in "nh".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition.clone();
        last_update = Some(update);
    }
    let final_candidates = &last_update.unwrap().candidates;
    assert!(final_candidates.iter().any(|c| c.text == "\u{4f60}\u{597d}"),
        "abbreviation nh should produce nihao, got top5: {:?}",
        final_candidates.iter().take(5).map(|c| &c.text).collect::<Vec<_>>());
}

#[test]
fn fuzzy_plus_abbreviation_zg_matches_zhongguo() {
    let config: SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  fuzzy_pinyin:\n    enabled: true\n"
    ).unwrap();
    let p = PipelineFactory::build(&config, None, Some(real_dict().clone()), None).unwrap();

    let mut comp = String::new();
    let mut last_update = None;
    for ch in "zg".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition.clone();
        last_update = Some(update);
    }
    let final_candidates = &last_update.unwrap().candidates;
    assert!(final_candidates.iter().any(|c| c.text.starts_with("\u{4e2d}\u{56fd}")),
        "fuzzy+abbreviation zg should produce zhongguo, got top5: {:?}",
        final_candidates.iter().take(5).map(|c| &c.text).collect::<Vec<_>>());
}
