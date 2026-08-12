//! End-to-end integration test: load example schemas from disk, parse sample
//! log lines through the streaming reader, and assert the produced records.

use std::path::PathBuf;
use tpt_telemetry_compiler::{CompiledSchema, Value};
use tpt_telemetry_core::Parser;

fn schema_dir() -> PathBuf {
    // examples/schemas lives at the workspace root, two levels up from this
    // crate's manifest dir.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("schemas")
}

fn load(name: &str) -> CompiledSchema {
    let path = schema_dir().join(name);
    let schema = tpt_telemetry_schema::load_file(path).expect("load schema");
    CompiledSchema::compile(&schema).expect("compile schema")
}

#[test]
fn cisco_asa_end_to_end() {
    let p = Parser::from_compiled(load("cisco_asa.tpt-log"));
    let rec = p
        .parse_line("%ASA-6-302013: Built inbound TCP connection")
        .expect("match");
    assert_eq!(rec.format, "CiscoASA");
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "severity").unwrap().value,
        Value::Enum(6)
    );
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "msg_id").unwrap().value,
        Value::Int(302013)
    );
    // message is redacted (masked).
    let msg = rec.fields.iter().find(|f| f.name == "message").unwrap();
    assert!(matches!(msg.value, Value::OwnedString(_)));
    assert!(msg.value.as_str().unwrap().starts_with("***"));
}

#[test]
fn rfc5424_end_to_end() {
    let p = Parser::from_compiled(load("rfc5424.tpt-log"));
    let rec = p
        .parse_line("<134>2024-03-11T08:22:01Z edge-fw01 sshd")
        .expect("match");
    assert_eq!(rec.format, "RFC5424");
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "pri").unwrap().value,
        Value::Int(134)
    );
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "host").unwrap().value,
        Value::Str("edge-fw01")
    );
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "app").unwrap().value,
        Value::Str("sshd")
    );
}

#[test]
fn cef_end_to_end() {
    let p = Parser::from_compiled(load("cef.tpt-log"));
    let rec = p
        .parse_line("CEF:0|CyberArk|Vault|12.6|100|Authentication failed|5|src=10.0.0.5")
        .expect("match");
    assert_eq!(rec.format, "CEF");
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "vendor").unwrap().value,
        Value::Str("CyberArk")
    );
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "signature").unwrap().value,
        Value::Str("100")
    );
    assert_eq!(
        rec.fields.iter().find(|f| f.name == "severity").unwrap().value,
        Value::Enum(5)
    );
}

#[test]
fn mixed_sample_log_streams_and_dispatches() {
    let asa = Parser::from_compiled(load("cisco_asa.tpt-log"));
    let rfc = Parser::from_compiled(load("rfc5424.tpt-log"));
    let cef = Parser::from_compiled(load("cef.tpt-log"));

    let path = schema_dir()
        .join("..")
        .join("samples")
        .join("mixed.log");
    let data = std::fs::read(&path).expect("read sample log");

    let mut r = tpt_telemetry_core::StreamReader::new(std::io::Cursor::new(&data[..]));
    let mut formats: std::collections::HashSet<String> = Default::default();
    while let Some(line) = r.next_line() {
        let s = std::str::from_utf8(line).unwrap();
        if let Some(rec) = asa
            .parse_line(s)
            .or_else(|| rfc.parse_line(s))
            .or_else(|| cef.parse_line(s))
        {
            formats.insert(rec.format.to_string());
        }
    }
    // All three formats should appear in the mixed sample.
    assert!(formats.contains("CiscoASA"));
    assert!(formats.contains("RFC5424"));
    assert!(formats.contains("CEF"));
}
