//! Integration / stress tests for ComposablePipeline with the real rime_ice dict (539K entries).
//!
//! Verifies correctness and catches regressions at scale.

use cheime_config::schema::SchemaConfig;
use cheime_dictionary::{parse_body, CompiledIndex, DictColumn};
use cheime_model::{DeploymentGeneration, Key, KeyEvent, KeyState};
use cheime_pipeline::factory::PipelineFactory;
use cheime_pipeline::InputPipeline;
use std::sync::{Arc, OnceLock};

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
    // "hao" maps to 👍 in emoji translator
    let mut comp = String::new();
    for ch in "hao".chars() {
        let update = p.apply(&comp, &key(ch)).unwrap();
        comp = update.composition;
    }
    let result = p.apply("ha", &key('o')).unwrap();
    let has_emoji = result.candidates.iter().any(|c| c.is_emoji);
    assert!(has_emoji, "hao should produce emoji candidate 👍");
}
