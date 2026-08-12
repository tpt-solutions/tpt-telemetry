//! Throughput / load harness: parses a large generated corpus of log lines and
//! reports sustained lines/sec (the Phase 8 "1M lines/sec/core" reference target).
//! Uses Criterion's element throughput so results read directly as lines/sec.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use std::io::Cursor;
use tpt_telemetry_core::{Parser, StreamReader};

const ASA: &str = r#"
    format CiscoASA {
      pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
      coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
    }
"#;

const LINE: &str = "%ASA-6-302013: Built inbound TCP connection";

/// Number of lines in the in-memory corpus fed to the load harness.
const CORPUS_LINES: usize = 4_096;

fn make_corpus() -> String {
    let mut s = String::with_capacity(LINE.len() * CORPUS_LINES);
    for _ in 0..CORPUS_LINES {
        s.push_str(LINE);
        s.push('\n');
    }
    s
}

fn bench_throughput(c: &mut Criterion) {
    let p = Parser::new(tpt_telemetry_schema::parse(ASA).unwrap()).unwrap();
    let corpus = make_corpus();

    {
        let mut g = c.benchmark_group("core_load_parse_corpus");
        g.throughput(Throughput::Elements(CORPUS_LINES as u64));
        g.bench_function("parse", |b| {
            b.iter_batched(
                || (),
                |_| {
                    for line in black_box(&corpus).lines() {
                        black_box(&p).parse_line(line);
                    }
                },
                BatchSize::SmallInput,
            )
        });
        g.finish();
    }

    {
        let mut g = c.benchmark_group("core_streamreader_framing");
        g.throughput(Throughput::Elements(CORPUS_LINES as u64));
        g.bench_function("frame", |b| {
            b.iter_batched(
                || (),
                |_| {
                    let mut r = StreamReader::new(Cursor::new(black_box(&corpus[..])));
                    let mut count = 0usize;
                    while let Some(_l) = r.next_line() {
                        count += 1;
                    }
                    black_box(count);
                },
                BatchSize::SmallInput,
            )
        });
        g.finish();
    }
}

criterion_group!(benches, bench_throughput);
criterion_main!(benches);
