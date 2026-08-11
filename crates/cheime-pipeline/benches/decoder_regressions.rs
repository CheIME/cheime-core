//! Decoder benchmarks for ambiguity regressions and representative long input.

use cheime_dictionary::{CompiledIndex, DictEntry};
use cheime_model::DeploymentGeneration;
use cheime_pipeline::Segmentor;
use cheime_pipeline::decoder::Decoder;
use cheime_pipeline::segmentor::PinyinSegmentor;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;

fn decoder() -> Decoder {
    let entries = [
        ("原神", "yuan shen", 300),
        ("现实", "xian shi", 300),
        ("中国", "zhong guo", 300),
        ("我", "wo", 100),
        ("明天", "ming tian", 200),
        ("要", "yao", 100),
        ("去", "qu", 100),
        ("北京", "bei jing", 200),
        ("工作", "gong zuo", 200),
        ("然后", "ran hou", 200),
        ("西安", "xi an", 200),
        ("参观", "can guan", 200),
    ]
    .into_iter()
    .map(|(text, code, weight)| DictEntry {
        text: text.to_owned(),
        code: code.to_owned(),
        weight: Some(weight),
        stem: None,
    })
    .collect();
    Decoder::new(vec![Arc::new(CompiledIndex::build(
        entries,
        DeploymentGeneration::new(1),
    ))])
}

fn bench_decoder_regressions(criterion: &mut Criterion) {
    let decoder = decoder();
    let segmentor = PinyinSegmentor::new();
    let mut group = criterion.benchmark_group("decoder/regressions");

    for (name, input, expected_text) in [
        ("yuanshen", "yuanshen", "原神"),
        ("xianshi", "xianshi", "现实"),
        ("zhongguo", "zhongguo", "中国"),
        (
            "long_sentence",
            "womingtianyaoqubeijinggongzuoranhouquxi'ancanguan",
            "我明天要去北京工作然后去西安参观",
        ),
    ] {
        let graph = segmentor.segment(input);
        assert!(
            decoder
                .decode(input, &graph)
                .iter()
                .any(|candidate| candidate.complete && candidate.display.text == expected_text)
        );
        group.bench_with_input(name, &(input, graph), |bencher, (input, graph)| {
            bencher.iter(|| black_box(decoder.decode(black_box(input), black_box(graph))));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_decoder_regressions);
criterion_main!(benches);
