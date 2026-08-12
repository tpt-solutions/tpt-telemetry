//! End-to-end pipeline benchmark: schema compile + runtime `Parser::parse_line`
//! dispatch over a representative Cisco ASA line.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tpt_telemetry_core::Parser;
use tpt_telemetry_schema::parse;

const ASA: &str = r#"
    format CiscoASA {
      pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
      coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
    }
"#;

const LINE: &str = "%ASA-6-302013: Built inbound TCP connection";

fn bench_e2e(c: &mut Criterion) {
    let p = Parser::new(parse(ASA).unwrap()).unwrap();
    c.bench_function("core_parse_line_e2e", |b| {
        b.iter(|| black_box(&p).parse_line(black_box(LINE)))
    });
}

criterion_group!(benches, bench_e2e);
criterion_main!(benches);
