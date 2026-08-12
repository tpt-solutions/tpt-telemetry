//! Criterion benchmarks comparing baseline vs SIMD scan hot paths.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tpt_grok_engine::Grok;

const SAMPLE: &str = "%ASA-3-106001: connection denied from 192.168.1.10 to 10.0.0.1";
const MISS: &str = "this line has nothing to do with the pattern at all, no anchor here";

fn bench_grok(c: &mut Criterion) {
    let g = Grok::new("%ASA-%{INT:severity}-%{NUMBER:msg_id}: %{GREEDYDATA:message}").unwrap();

    c.bench_function("grok_baseline_match", |b| {
        b.iter(|| black_box(&g).find(black_box(SAMPLE)))
    });

    c.bench_function("grok_simd_scan_match", |b| {
        b.iter(|| black_box(&g).scan(black_box(SAMPLE)))
    });

    c.bench_function("grok_simd_scan_miss", |b| {
        b.iter(|| black_box(&g).scan(black_box(MISS)))
    });

    let ip = Grok::new("%{IP:client} %{WORD:action} %{NUMBER:bytes:int}").unwrap();
    c.bench_function("grok_ip_compound", |b| {
        b.iter(|| black_box(&ip).scan(black_box("203.0.113.7 accepted 4096")))
    });
}

criterion_group!(benches, bench_grok);
criterion_main!(benches);
