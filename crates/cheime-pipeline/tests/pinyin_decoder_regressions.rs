use cheime_dictionary::{CompiledIndex, DictEntry};
use cheime_model::DeploymentGeneration;
use cheime_pipeline::Segmentor;
use cheime_pipeline::decoder::Decoder;
use cheime_pipeline::segmentor::PinyinSegmentor;
use std::sync::Arc;

fn decoder(entries: &[(&str, &str, i64)]) -> Decoder {
    let entries = entries
        .iter()
        .map(|(text, code, weight)| DictEntry {
            text: (*text).to_owned(),
            code: (*code).to_owned(),
            weight: Some(*weight),
            stem: None,
        })
        .collect();
    Decoder::new(vec![Arc::new(CompiledIndex::build(
        entries,
        DeploymentGeneration::new(1),
    ))])
}

#[test]
fn yuanshen_primary_path_uses_yuan_shen() {
    let codes: Vec<_> = PinyinSegmentor::new()
        .segment("yuanshen")
        .primary_path()
        .into_iter()
        .map(|segment| segment.code)
        .collect();

    assert_eq!(codes, ["yuan", "shen"]);
}

#[test]
fn yuanshen_uses_canonical_code() {
    let decoder = decoder(&[
        ("原神", "yuan shen", 300),
        ("鱼", "yu", 90),
        ("安", "an", 80),
        ("社", "she", 70),
        ("嗯", "n", 60),
    ]);
    let graph = PinyinSegmentor::new().segment("yuanshen");
    let candidate = decoder
        .decode("yuanshen", &graph)
        .into_iter()
        .find(|candidate| candidate.display.text == "原神")
        .unwrap();

    assert_eq!(candidate.canonical_code, "yuan shen");
    assert!(candidate.complete);
    assert!(candidate.exact_phrase);
    assert!(!candidate.completion);
}

#[test]
fn long_sentence_is_composed_from_inline_lexemes() {
    let decoder = decoder(&[
        ("我", "wo", 100),
        ("明天", "ming tian", 200),
        ("要", "yao", 100),
        ("去", "qu", 100),
        ("北京", "bei jing", 200),
        ("工作", "gong zuo", 200),
        ("然后", "ran hou", 200),
        ("西安", "xi an", 200),
        ("参观", "can guan", 200),
    ]);
    let input = "womingtianyaoqubeijinggongzuoranhouquxi'ancanguan";
    let graph = PinyinSegmentor::new().segment(input);
    let candidate = decoder
        .decode(input, &graph)
        .into_iter()
        .find(|candidate| candidate.display.text == "我明天要去北京工作然后去西安参观")
        .unwrap();

    assert_eq!(
        candidate.canonical_code,
        "wo ming tian yao qu bei jing gong zuo ran hou qu xi an can guan"
    );
    assert!(candidate.complete);
    assert!(!candidate.exact_phrase);
    assert!(!candidate.completion);
    assert_eq!(candidate.lexemes.len(), 10);
}

#[test]
fn ambiguous_long_sentence_preserves_canonical_code() {
    let decoder = decoder(&[
        ("西安", "xi an", 300),
        ("是", "shi", 100),
        ("一个", "yi ge", 200),
        ("现代", "xian dai", 200),
        ("城市", "cheng shi", 200),
        ("先", "xian", 90),
        ("时", "shi", 90),
    ]);
    let input = "xianshiyigexiandaichengshi";
    let graph = PinyinSegmentor::new().segment(input);
    let candidate = decoder
        .decode(input, &graph)
        .into_iter()
        .find(|candidate| candidate.display.text == "西安是一个现代城市")
        .unwrap();

    assert_eq!(
        candidate.canonical_code,
        "xi an shi yi ge xian dai cheng shi"
    );
    assert!(candidate.complete);
    assert!(!candidate.exact_phrase);
    assert!(!candidate.completion);
    assert_eq!(candidate.lexemes.len(), 5);
}
