//! Criterion benchmark for the schema compiler: compile-time cost of lowering a
//! `.tpt-log` schema into a `CompiledSchema`, plus steady-state per-line parse
//! throughput of the generated zero-copy matcher.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tpt_telemetry_compiler::CompiledSchema;
use tpt_telemetry_schema::parse;

const ASA: &str = r#"
    format CiscoASA {
      pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
      coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
    }
"#;

const LINE: &str = "%ASA-6-302013: Built inbound TCP connection";

fn bench_compile(c: &mut Criterion) {
    c.bench_function("compiler_compile_schema", |b| {
        b.iter(|| {
            let s = parse(black_box(ASA)).unwrap();
            CompiledSchema::compile(&s).unwrap()
        })
    });

    let schema = parse(ASA).unwrap();
    let cs = CompiledSchema::compile(&schema).unwrap();
    c.bench_function("compiler_parse_line", |b| {
        b.iter(|| black_box(&cs).parse_line(black_box(LINE)))
    });
}

criterion_group!(benches, bench_compile);
criterion_main!(benches);
