//! Benchmarks for wire framing: encode/decode of length-prefixed MessagePack frames.
//!
//! Covers the hot IPC path — every key event and snapshot crosses this boundary.

use cheime_model::{
    Candidate, CandidateId, CandidateSnapshot, ClientInstanceId, DeploymentGeneration, Key,
    KeyEvent, KeyState, Revision, Sequence, SessionEpoch, SessionId, SessionStatus,
};
use cheime_protocol::{EngineMessage, FrontendMessage, MessageHeader};
use cheime_wire::{FramedReader, FramedWriter, MessageCodec};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn key_message() -> FrontendMessage {
    FrontendMessage::KeyCommand {
        header: MessageHeader {
            protocol_version: 1,
            client: ClientInstanceId::new(1),
            session: SessionId::new(2),
            epoch: SessionEpoch::new(3),
            sequence: Sequence::new(42),
            revision: Revision::new(7),
            deployment: DeploymentGeneration::new(0),
        },
        event: KeyEvent {
            key: Key::Character('n'),
            state: KeyState::default(),
        },
    }
}

fn snapshot_message(nc: usize) -> EngineMessage {
    let candidates: Vec<Candidate> = (0..nc)
        .map(|i| Candidate {
            id: CandidateId::new(i as u64 + 1),
            text: format!("候选{}", i),
            annotation: Some(format!("pinyin{}", i)),
            source: String::from("bench"),
            is_emoji: false,
        })
        .collect();
    EngineMessage::CandidateSnapshot {
        header: MessageHeader {
            protocol_version: 1,
            client: ClientInstanceId::new(1),
            session: SessionId::new(2),
            epoch: SessionEpoch::new(3),
            sequence: Sequence::new(42),
            revision: Revision::new(7),
            deployment: DeploymentGeneration::new(0),
        },
        snapshot: CandidateSnapshot {
            epoch: SessionEpoch::new(3),
            revision: Revision::new(7),
            deployment: DeploymentGeneration::new(0),
            preedit: String::from("zhongguo"),
            cursor: 8,
            candidates,
            highlighted: Some(CandidateId::new(1)),
            status: SessionStatus::Composing,
            page_size: 9,
            page: 0,
        },
    }
}

fn bench_encode_key_message(c: &mut Criterion) {
    let msg = key_message();
    let mut buf = vec![0u8; 4096];
    let codec = MessageCodec::new(MessageCodec::DEFAULT_MAX);
    c.bench_function("wire/encode_key_message", |b| {
        b.iter(|| FramedWriter::write_frame(black_box(&mut buf), &codec, black_box(&msg)).unwrap())
    });
}

fn bench_encode_snapshot_10(c: &mut Criterion) {
    let msg = snapshot_message(10);
    let mut buf = vec![0u8; 4096];
    let codec = MessageCodec::new(MessageCodec::DEFAULT_MAX);
    c.bench_function("wire/encode_snapshot_10_candidates", |b| {
        b.iter(|| FramedWriter::write_frame(black_box(&mut buf), &codec, black_box(&msg)).unwrap())
    });
}

fn bench_encode_snapshot_50(c: &mut Criterion) {
    let msg = snapshot_message(50);
    let mut buf = vec![0u8; 16384];
    let codec = MessageCodec::new(MessageCodec::DEFAULT_MAX);
    c.bench_function("wire/encode_snapshot_50_candidates", |b| {
        b.iter(|| FramedWriter::write_frame(black_box(&mut buf), &codec, black_box(&msg)).unwrap())
    });
}

fn bench_decode_frame_header(c: &mut Criterion) {
    let msg = key_message();
    let codec = MessageCodec::new(MessageCodec::DEFAULT_MAX);
    let mut buf = vec![0u8; 4096];
    let _ = FramedWriter::write_frame(&mut buf, &codec, &msg).unwrap();
    c.bench_function("wire/decode_frame_header", |b| {
        b.iter(|| {
            FramedReader::read_frame(black_box(&buf), black_box(codec.max_size()))
                .unwrap()
                .is_some()
        })
    });
}

fn bench_roundtrip_key_message(c: &mut Criterion) {
    let msg = key_message();
    let mut buf = vec![0u8; 4096];
    let codec = MessageCodec::new(MessageCodec::DEFAULT_MAX);
    c.bench_function("wire/roundtrip_key_message", |b| {
        b.iter(|| {
            let _ =
                FramedWriter::write_frame(black_box(&mut buf), &codec, black_box(&msg)).unwrap();
            let (payload_start, payload_len) =
                FramedReader::read_frame(black_box(&buf), black_box(codec.max_size()))
                    .unwrap()
                    .unwrap();
            let _: FrontendMessage =
                rmp_serde::from_slice(black_box(&buf[payload_start..payload_start + payload_len]))
                    .unwrap();
        })
    });
}

fn bench_roundtrip_snapshot_50(c: &mut Criterion) {
    let msg = snapshot_message(50);
    let mut buf = vec![0u8; 16384];
    let codec = MessageCodec::new(MessageCodec::DEFAULT_MAX);
    c.bench_function("wire/roundtrip_snapshot_50_candidates", |b| {
        b.iter(|| {
            let _ =
                FramedWriter::write_frame(black_box(&mut buf), &codec, black_box(&msg)).unwrap();
            let (payload_start, payload_len) =
                FramedReader::read_frame(black_box(&buf), black_box(codec.max_size()))
                    .unwrap()
                    .unwrap();
            let _: EngineMessage =
                rmp_serde::from_slice(black_box(&buf[payload_start..payload_start + payload_len]))
                    .unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_encode_key_message,
    bench_encode_snapshot_10,
    bench_encode_snapshot_50,
    bench_decode_frame_header,
    bench_roundtrip_key_message,
    bench_roundtrip_snapshot_50,
);
criterion_main!(benches);
