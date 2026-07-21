//! Benchmarks for InputPipeline: key-to-candidate latency.
//!
//! Covers the real-time path (§17.1): key event → composition update → candidate lookup.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use cheime_pipeline::{BuiltinPipeline, InputPipeline};
use cheime_model::{Key, KeyEvent, KeyState};

fn char_key(ch: char) -> KeyEvent {
    KeyEvent { key: Key::Character(ch), state: KeyState::default() }
}

fn backspace_key() -> KeyEvent {
    KeyEvent { key: Key::Backspace, state: KeyState::default() }
}

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
        ("", 'z'), ("z", 'h'), ("zh", 'o'), ("zho", 'n'),
        ("zhon", 'g'), ("zhong", 'g'), ("zhongg", 'u'), ("zhonggu", 'o'),
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

criterion_group!(benches, bench_char_append, bench_char_continue, bench_backspace, bench_typing_zhongguo);
criterion_main!(benches);
