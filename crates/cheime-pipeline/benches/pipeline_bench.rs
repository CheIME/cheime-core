//! Benchmarks for InputPipeline: key-to-candidate latency.
//!
//! Covers the real-time path (§17.1): key event → composition update → candidate lookup.
//!
//! Uses the rime_ice 539K-entry dictionary for realistic stress benchmarking.
//! Data loaded once via `OnceLock` to amortize I/O.

use cheime_config::schema::{EngineConfig, SchemaConfig, SegmentorConfig};
use cheime_dictionary::{CompiledIndex, DictColumn, parse_body};
use cheime_model::{DeploymentGeneration, Key, KeyEvent, KeyState};
use cheime_pipeline::factory::PipelineFactory;
use cheime_pipeline::{BuiltinPipeline, InputPipeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::sync::{Arc, OnceLock};

fn char_key(ch: char) -> KeyEvent {
    KeyEvent {
        key: Key::Character(ch),
        state: KeyState::default(),
    }
}

fn backspace_key() -> KeyEvent {
    KeyEvent {
        key: Key::Backspace,
        state: KeyState::default(),
    }
}

// ── Tiny builtin pipeline (original fast-path benches) ───────────────

fn pinyin_pipeline() -> BuiltinPipeline {
    BuiltinPipeline::new([
        ("n".into(), "嗯".into(), 10),
        ("ni".into(), "你".into(), 100),
        ("ni".into(), "尼".into(), 50),
        ("nihao".into(), "你好".into(), 200),
        ("hao".into(), "好".into(), 90),
        ("zhong".into(), "中".into(), 100),
        ("zhong".into(), "重".into(), 70),
        ("guo".into(), "国".into(), 100),
        ("zhongguo".into(), "中国".into(), 300),
    ])
}

fn bench_char_append(c: &mut Criterion) {
    let p = pinyin_pipeline();
    let key = char_key('n');
    c.bench_function("pipeline/char_append", |b| {
        b.iter(|| p.apply(black_box(""), black_box(&key)).unwrap())
    });
}

fn bench_char_continue(c: &mut Criterion) {
    let p = pinyin_pipeline();
    let key = char_key('i');
    c.bench_function("pipeline/char_continue", |b| {
        b.iter(|| p.apply(black_box("n"), black_box(&key)).unwrap())
    });
}

fn bench_backspace(c: &mut Criterion) {
    let p = pinyin_pipeline();
    let key = backspace_key();
    c.bench_function("pipeline/backspace", |b| {
        b.iter(|| p.apply(black_box("ni"), black_box(&key)).unwrap())
    });
}

fn bench_typing_zhongguo(c: &mut Criterion) {
    let p = pinyin_pipeline();
    let steps: Vec<(&str, char)> = vec![
        ("", 'z'),
        ("z", 'h'),
        ("zh", 'o'),
        ("zho", 'n'),
        ("zhon", 'g'),
        ("zhong", 'g'),
        ("zhongg", 'u'),
        ("zhonggu", 'o'),
    ];
    c.bench_function("pipeline/typing_zhongguo_8keys", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for (comp, ch) in &steps {
                let k = char_key(*ch);
                let update = p.apply(black_box(comp), black_box(&k)).unwrap();
                total = total.wrapping_add(update.candidates.len());
            }
            black_box(total);
        })
    });
}

// ── Real rime_ice pipeline (539K entries) ────────────────────────────

static RIME_ICE_PIPELINE: OnceLock<Arc<dyn InputPipeline>> = OnceLock::new();

fn rime_ice_pipeline() -> &'static Arc<dyn InputPipeline> {
    RIME_ICE_PIPELINE.get_or_init(|| {
        let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
        let body = if let Some(p) = raw.find("\n...\n") {
            &raw[p + 5..]
        } else {
            raw
        };
        let cols = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];
        let entries = parse_body(body, cols).expect("failed to parse rime_ice body");
        eprintln!("rime_ice pipeline: {} entries loaded", entries.len());
        let idx = Arc::new(CompiledIndex::build(entries, DeploymentGeneration::new(1)));

        let config = SchemaConfig {
            schema_version: 1,
            engine: EngineConfig {
                segmentors: vec![SegmentorConfig::PinyinSyllable],
                ..Default::default()
            },
            ..Default::default()
        };

        let pipeline = PipelineFactory::build(&config, None, Some(idx), None)
            .expect("failed to build rime_ice pipeline");
        Arc::new(pipeline)
    })
}

// ── Real pipeline benches ────────────────────────────────────────────

/// Type "zhongguo" — 8 keystrokes through the real 539K pipeline.
/// This is the real-world user-facing latency path.
fn bench_real_typing_zhongguo(c: &mut Criterion) {
    let p = rime_ice_pipeline();
    let steps: Vec<(&str, char)> = vec![
        ("", 'z'),
        ("z", 'h'),
        ("zh", 'o'),
        ("zho", 'n'),
        ("zhon", 'g'),
        ("zhong", 'g'),
        ("zhongg", 'u'),
        ("zhonggu", 'o'),
    ];
    c.bench_function("pipeline/real_typing_zhongguo", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for (comp, ch) in &steps {
                let k = char_key(*ch);
                let update = p.apply(black_box(comp), black_box(&k)).unwrap();
                total = total.wrapping_add(update.candidates.len());
            }
            black_box(total);
        })
    });
}

/// Type "zhonghuarenmingongheguo" — 22+ character stress test through
/// the real pipeline, exercising long composition segmentation.
fn bench_real_typing_zhonghuarenmingongheguo(c: &mut Criterion) {
    let p = rime_ice_pipeline();
    let s = "zhonghuarenmingongheguo";
    let steps: Vec<(&str, char)> = {
        let mut v = Vec::new();
        for i in 0..s.chars().count() {
            v.push((
                &s[..s.char_indices().nth(i).map(|(j, _)| j).unwrap_or(s.len())],
                s.chars().nth(i).unwrap(),
            ));
        }
        v
    };
    c.bench_function("pipeline/real_typing_zhonghuarenmingongheguo", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for (comp, ch) in &steps {
                let k = char_key(*ch);
                let update = p.apply(black_box(comp), black_box(&k)).unwrap();
                total = total.wrapping_add(update.candidates.len());
            }
            black_box(total);
        })
    });
}

/// Simulate 10 concurrent sessions typing "ni hao" through the real
/// pipeline. Exercises shared-index contention under concurrent access.
fn bench_real_concurrent_lookups(c: &mut Criterion) {
    let p = rime_ice_pipeline();
    // Each "session" types "nihao" one character at a time.
    let steps: Vec<(&str, char)> = vec![
        ("", 'n'),
        ("n", 'i'),
        ("ni", 'h'),
        ("nih", 'a'),
        ("niha", 'o'),
    ];
    c.bench_function("pipeline/real_concurrent_lookups", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for _ in 0..10 {
                let mut comp = String::new();
                for (_, ch) in &steps {
                    let k = char_key(*ch);
                    let update = p.apply(black_box(&comp), black_box(&k)).unwrap();
                    comp = update.composition;
                    total = total.wrapping_add(update.candidates.len());
                }
            }
            black_box(total);
        })
    });
}

/// Bound decoder work for completion, common words, ambiguity, and long input.
fn bench_real_decoder_inputs(c: &mut Criterion) {
    let pipeline = rime_ice_pipeline();
    let mut group = c.benchmark_group("pipeline/real_decode");
    for input in [
        "nih",
        "nihao",
        "xianshi",
        "woshiyigemingtianyaoqubeijinggongzuodechengxuyuan",
    ] {
        group.bench_with_input(input, input, |b, input| {
            b.iter(|| {
                let candidates = pipeline.refresh(black_box(input)).unwrap();
                black_box(candidates.len())
            })
        });
    }
    group.finish();
}

// ── Criterion groups ─────────────────────────────────────────────────

criterion_group!(
    tiny,
    bench_char_append,
    bench_char_continue,
    bench_backspace,
    bench_typing_zhongguo,
);

criterion_group!(
    real,
    bench_real_typing_zhongguo,
    bench_real_typing_zhonghuarenmingongheguo,
    bench_real_concurrent_lookups,
    bench_real_decoder_inputs,
);

criterion_main!(tiny, real);
