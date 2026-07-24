use super::*;
use crate::{InputPipeline, factory::PipelineFactory};
use cheime_config::schema::SchemaConfig;
use cheime_dictionary::{CompiledIndex, DictEntry};
use cheime_model::{DeploymentGeneration, Key, KeyEvent};
use std::sync::Arc;

fn test_index() -> Arc<CompiledIndex> {
    let entries = vec![
        DictEntry {
            text: "你".into(),
            code: "ni".into(),
            weight: Some(100),
            stem: None,
        },
        DictEntry {
            text: "好".into(),
            code: "hao".into(),
            weight: Some(100),
            stem: None,
        },
    ];
    Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1)))
}

fn segments(codes: &[&str]) -> Vec<CodeSegment> {
    codes
        .iter()
        .map(|code| CodeSegment {
            code: (*code).to_owned(),
            tag: "pinyin".to_owned(),
        })
        .collect()
}

#[test]
fn dict_translator_returns_candidates() {
    let translator = DictTranslator::new("test", test_index());
    let segment = CodeSegment {
        code: "ni".into(),
        tag: "pinyin".into(),
    };
    let candidates = translator.translate(&[segment]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "你");
}

#[test]
fn passthrough_returns_code_as_text() {
    let translator = PassthroughTranslator;
    let segment = CodeSegment {
        code: "hello".into(),
        tag: "unknown".into(),
    };
    let candidates = translator.translate(&[segment]);
    assert_eq!(candidates[0].text, "hello");
}

#[test]
fn multi_segment_lookup_keeps_per_segment_concatenation_with_prefix_matches() {
    let entries = vec![
        DictEntry {
            text: "你".into(),
            code: "ni".into(),
            weight: Some(100),
            stem: None,
        },
        DictEntry {
            text: "候选短语".into(),
            code: "ni ni ni ni ni ma".into(),
            weight: Some(200),
            stem: None,
        },
    ];
    let translator = DictTranslator::new(
        "test",
        Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1))),
    );

    let candidates = translator.translate(&segments(&["ni", "ni", "ni", "ni", "ni"]));

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.text == "你你你你你")
    );
}

#[test]
fn exact_phrase_precedes_higher_weight_prefix_completion() {
    let entries = vec![
        DictEntry {
            text: "精确".into(),
            code: "ni hao".into(),
            weight: Some(1),
            stem: None,
        },
        DictEntry {
            text: "补全".into(),
            code: "ni hao ma".into(),
            weight: Some(1000),
            stem: None,
        },
    ];
    let translator = DictTranslator::new(
        "test",
        Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1))),
    );

    let candidates = translator.translate(&segments(&["ni", "hao"]));

    assert_eq!(candidates[0].text, "精确");
}

#[test]
fn full_pipeline_combines_repeated_syllables_from_single_character_entries() {
    let config: SchemaConfig = serde_yaml::from_str(
        "schema_version: 1\nengine:\n  segmentors:\n    - type: pinyin_syllable\n",
    )
    .unwrap();
    let pipeline = PipelineFactory::build(&config, None, Some(test_index()), None).unwrap();

    let mut composition = String::new();
    let mut update = None;
    for character in "ninininini".chars() {
        let result = pipeline
            .apply(
                &composition,
                &KeyEvent {
                    key: Key::Character(character),
                    state: Default::default(),
                },
            )
            .unwrap();
        composition = result.composition.clone();
        update = Some(result);
    }

    let update = update.expect("typing repeated ni produces an update");
    assert!(
        update
            .candidates
            .iter()
            .any(|candidate| candidate.text == "你你你你你")
    );
    assert_eq!(
        update.candidates.len(),
        update
            .candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
}
