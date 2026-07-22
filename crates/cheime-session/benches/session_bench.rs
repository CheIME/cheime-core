//! Benchmarks for Session: end-to-end key→message throughput.
//!
//! Covers the full Session::handle path: header validation, pipeline dispatch,
//! action generation, and snapshot construction (§17.4).

use cheime_model::{
    CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Key, KeyEvent, KeyState,
    Revision, Sequence, SessionEpoch, SessionId,
};
use cheime_pipeline::BuiltinPipeline;
use cheime_protocol::{FrontendMessage, MessageHeader};
use cheime_session::Session;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn make_header() -> MessageHeader {
    MessageHeader {
        protocol_version: CORE_PROTOCOL_VERSION,
        client: ClientInstanceId::new(1),
        session: SessionId::new(2),
        epoch: SessionEpoch::new(3),
        sequence: Sequence::new(0),
        revision: Revision::new(0),
        deployment: DeploymentGeneration::new(4),
    }
}

fn key_message(seq: u64, rev: u64, key: Key) -> FrontendMessage {
    let mut h = make_header();
    h.sequence = Sequence::new(seq);
    h.revision = Revision::new(rev);
    FrontendMessage::KeyCommand {
        header: h,
        event: KeyEvent {
            key,
            state: KeyState::default(),
        },
    }
}

fn pinyin_pipeline() -> BuiltinPipeline {
    BuiltinPipeline::new([
        ("n".into(), "嗯".into(), 10),
        ("ni".into(), "你".into(), 100),
        ("nihao".into(), "你好".into(), 200),
        ("hao".into(), "好".into(), 90),
        ("zhong".into(), "中".into(), 100),
        ("guo".into(), "国".into(), 100),
        ("zhongguo".into(), "中国".into(), 300),
    ])
}

fn bench_first_key(c: &mut Criterion) {
    let pipeline = pinyin_pipeline();
    let msg = key_message(1, 0, Key::Character('n'));
    c.bench_function("session/first_key", |b| {
        b.iter(|| {
            let mut s = Session::new(make_header(), pipeline.clone());
            black_box(s.handle(black_box(msg.clone())).unwrap())
        })
    });
}

fn bench_typing_zhongguo(c: &mut Criterion) {
    let chars: Vec<char> = "zhongguo".chars().collect();
    c.bench_function("session/typing_zhongguo_8keys", |b| {
        b.iter(|| {
            let pipeline = pinyin_pipeline();
            let mut s = Session::new(make_header(), pipeline);
            for (i, &ch) in chars.iter().enumerate() {
                let msg = key_message(i as u64 + 1, i as u64, Key::Character(ch));
                black_box(s.handle(black_box(msg)).unwrap());
            }
        })
    });
}

fn bench_commit_roundtrip(c: &mut Criterion) {
    c.bench_function("session/commit_ni", |b| {
        b.iter(|| {
            let pipeline = pinyin_pipeline();
            let mut s = Session::new(make_header(), pipeline);
            s.handle(key_message(1, 0, Key::Character('n'))).unwrap();
            s.handle(key_message(2, 1, Key::Character('i'))).unwrap();
            black_box(s.handle(key_message(3, 2, Key::Enter)).unwrap())
        })
    });
}

criterion_group!(
    benches,
    bench_first_key,
    bench_typing_zhongguo,
    bench_commit_roundtrip
);
criterion_main!(benches);
