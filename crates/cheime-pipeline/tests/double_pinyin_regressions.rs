//! Cross-cutting double-pinyin regressions: raw spans, quanpin/flypy
//! decoder consistency, and prefix completion.

use cheime_config::schema::SchemaConfig;
use cheime_dictionary::{CompiledIndex, DictEntry};
use cheime_model::{DeploymentGeneration, Key, KeyEvent, KeyState};
use cheime_pipeline::decoder::ResolvedCandidate;
use cheime_pipeline::factory::PipelineFactory;
use cheime_pipeline::segmentation::InputSpan;
use cheime_pipeline::InputPipeline;
use std::sync::Arc;

fn key(ch: char) -> KeyEvent {
    KeyEvent {
        key: Key::Character(ch),
        state: KeyState::default(),
    }
}

fn index() -> Arc<CompiledIndex> {
    Arc::new(CompiledIndex::build(
        vec![
            DictEntry { text: "中".into(), code: "zhong".into(), weight: Some(100), stem: None },
            DictEntry { text: "国".into(), code: "guo".into(), weight: Some(100), stem: None },
            DictEntry { text: "中国".into(), code: "zhong guo".into(), weight: Some(500), stem: None },
            DictEntry { text: "中国人".into(), code: "zhong guo ren".into(), weight: Some(600), stem: None },
            DictEntry { text: "宅".into(), code: "zhai".into(), weight: Some(80), stem: None },
            DictEntry { text: "张".into(), code: "zhang".into(), weight: Some(90), stem: None },
        ],
        DeploymentGeneration::new(1),
    ))
}

fn flypy_pipeline(index: Arc<CompiledIndex>) -> impl InputPipeline {
    let config: SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  input:\n    type: double_pinyin\n    scheme:\n      preset: flypy\n  translators:\n    - type: dict\n      dictionary: main\n",
    )
    .unwrap();
    PipelineFactory::build(&config, None, Some(index), None).unwrap()
}

fn quanpin_pipeline(index: Arc<CompiledIndex>) -> impl InputPipeline {
    let config: SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n  translators:\n    - type: dict\n      dictionary: main\n",
    )
    .unwrap();
    PipelineFactory::build(&config, None, Some(index), None).unwrap()
}

fn type_all(pipeline: &impl InputPipeline, keys: &str) -> Vec<ResolvedCandidate> {
    let mut composition = String::new();
    let mut update = None;
    for ch in keys.chars() {
        update = Some(pipeline.apply(&composition, &key(ch)).unwrap());
        composition = update.as_ref().unwrap().composition.clone();
    }
    update.unwrap().candidates
}

#[test]
fn flypy_vsgo_matches_quanpin_zhongguo() {
    let shared = index();
    let quanpin = type_all(&quanpin_pipeline(shared.clone()), "zhongguo");
    let flypy = type_all(&flypy_pipeline(shared.clone()), "vsgo");

    // Exact-match ranking must be scheme-independent: the decoder, ranker,
    // and dictionary are shared and see only canonical syllables. The
    // single-key Incomplete edge (v → "zh") is suppressed whenever a complete
    // pair exists at the same start (segment_pair_suppresses_mid_pair_incomplete),
    // so the top-5 candidate lists are strictly equal across schemes.
    let top_quanpin: Vec<String> = quanpin
        .iter()
        .take(5)
        .map(|c| c.display.text.clone())
        .collect();
    let top_flypy: Vec<String> = flypy
        .iter()
        .take(5)
        .map(|c| c.display.text.clone())
        .collect();
    assert_eq!(
        top_quanpin, top_flypy,
        "top-5 candidates must not depend on the input scheme"
    );
    let china = flypy.iter().find(|c| c.display.text == "中国").unwrap();
    assert_eq!(china.canonical_code, "zhong guo");
    assert_eq!(china.consumed, InputSpan::new(0, 4));
    assert!(china.complete);
}

#[test]
fn flypy_candidates_use_raw_spans() {
    let candidates = type_all(&flypy_pipeline(index()), "vsgo");
    let china = candidates.iter().find(|c| c.display.text == "中国").unwrap();
    assert_eq!(china.consumed, InputSpan::new(0, 4));
    let zhong = candidates.iter().find(|c| c.display.text == "中").unwrap();
    let guo = type_all(&flypy_pipeline(index()), "go");
    let guo = guo.iter().find(|c| c.display.text == "国").unwrap();
    assert_eq!(guo.consumed, InputSpan::new(0, 2));
}

#[test]
fn flypy_partial_input_offers_prefix_candidates() {
    // "v" → Incomplete edge canonical "zh" → prefix lookup
    let candidates = type_all(&flypy_pipeline(index()), "v");
    let texts: Vec<&str> = candidates.iter().map(|c| c.display.text.as_str()).collect();
    assert!(texts.contains(&"中"), "v must complete to 中 via zh prefix; got {texts:?}");
    assert!(texts.contains(&"张"), "v must complete to 张 via zh prefix; got {texts:?}");
}
