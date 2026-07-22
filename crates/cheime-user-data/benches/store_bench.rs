//! Benchmarks for UserStore: event recording and frequency lookups.
//!
//! Covers the user-data write path (§10) — learns must not block input.

use cheime_user_data::{UserEvent, UserStore};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_bulk_learn_1k(c: &mut Criterion) {
    let events: Vec<UserEvent> = (0..1000)
        .map(|i| {
            UserEvent::learn_word(
                "device-a",
                "quanpin",
                &format!("词{}", i),
                &format!("c{}", i),
            )
        })
        .collect();
    c.bench_function("user_data/bulk_learn_1k", |b| {
        b.iter(|| {
            let mut store = UserStore::new("bench-device");
            for event in &events {
                store.apply(black_box(event.clone()));
            }
        })
    });
}

fn bench_single_learn(c: &mut Criterion) {
    c.bench_function("user_data/single_learn", |b| {
        b.iter(|| {
            let mut store = UserStore::new("bench-device");
            store.apply(black_box(UserEvent::learn_word(
                "device-a", "quanpin", "测试", "ceshi",
            )))
        })
    });
}

fn bench_frequency_lookup(c: &mut Criterion) {
    let mut store = UserStore::new("bench-device");
    for i in 0..10_000 {
        store.apply(UserEvent::learn_word(
            "device-a",
            "quanpin",
            &format!("词{}", i % 200),
            &format!("c{}", i % 200),
        ));
    }
    c.bench_function("user_data/frequency_lookup", |b| {
        b.iter(|| black_box(store.frequency(black_box("quanpin"), black_box("词42"))))
    });
}

fn bench_query_by_code(c: &mut Criterion) {
    let mut store = UserStore::new("bench-device");
    for i in 0..10_000 {
        store.apply(UserEvent::learn_word(
            "device-a",
            "quanpin",
            &format!("词{}", i % 200),
            &format!("c{}", i % 50),
        ));
    }
    c.bench_function("user_data/query_by_code", |b| {
        b.iter(|| black_box(store.query(black_box("c42"))))
    });
}

criterion_group!(
    benches,
    bench_bulk_learn_1k,
    bench_single_learn,
    bench_frequency_lookup,
    bench_query_by_code
);
criterion_main!(benches);
