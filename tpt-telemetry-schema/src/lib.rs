//! `tpt-telemetry-schema` — the `.tpt-log` schema DSL.
//!
//! Defines the grammar, AST, and parser for log-format schemas, plus a curated
//! library of standard Grok patterns (Logstash base + ECS subset) so schemas can
//! reference `%{PATTERN:field}` captures exactly as in a Logstash pipeline.

pub mod ast;
pub mod parser;
pub mod patterns;

pub use ast::*;
pub use parser::{parse, SchemaError};

use std::path::Path;

/// Parse a `.tpt-log` schema from a file.
pub fn load_file(path: impl AsRef<Path>) -> Result<ast::Schema, SchemaError> {
    let text = std::fs::read_to_string(path).map_err(|e| SchemaError::Parse(e.to_string()))?;
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        format CiscoASA {
          pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";

          extract source_ip from message using regex "\b(?:\d{1,3}\.){3}\d{1,3}\b";
          coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
          redact message with mask;
        }
    "#;

    #[test]
    fn parses_format_block() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.formats.len(), 1);
        let f = &s.formats[0];
        assert_eq!(f.name, "CiscoASA");
        assert_eq!(f.redactions.len(), 1);
        assert!(matches!(f.redactions[0].mode, RedactMode::Mask));
    }

    #[test]
    fn parses_native_captures() {
        let s = parse(SAMPLE).unwrap();
        let f = &s.formats[0];
        let caps: Vec<&PatternCapture> = f
            .pattern
            .parts
            .iter()
            .filter_map(|p| match p {
                PatternPart::Capture(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(caps.len(), 3);
        assert_eq!(caps[0].name, "severity");
        assert_eq!(caps[0].ty, TypeName::Int);
        assert!(!caps[0].grok);
        assert_eq!(caps[2].name, "message");
        assert_eq!(caps[2].ty, TypeName::String);
    }

    #[test]
    fn parses_extract_and_enum() {
        let s = parse(SAMPLE).unwrap();
        let f = &s.formats[0];
        assert_eq!(f.extracts[0].field, "source_ip");
        assert_eq!(f.extracts[0].source, "message");
        assert!(f.extracts[0].regex.contains(r"\d{1,3}"));

        let co = &f.coercions[0];
        assert_eq!(co.field, "severity");
        match &co.target {
            CoercionTarget::Enum(v) => assert_eq!(v.len(), 8),
            other => panic!("expected enum, got {other:?}"),
        }
    }

    #[test]
    fn detects_duplicate_format() {
        let dup = "format A { pattern: \"x\"; } format A { pattern: \"y\"; }";
        assert!(parse(dup).is_err());
    }

    #[test]
    fn resolves_grok_capture() {
        let s = parse(r#"format G { pattern: "%{IP:client} %{NUMBER:bytes:int}"; }"#).unwrap();
        let caps: Vec<&PatternCapture> = s.formats[0]
            .pattern
            .parts
            .iter()
            .filter_map(|p| match p {
                PatternPart::Capture(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(caps.len(), 2);
        assert!(caps[0].grok);
        assert_eq!(caps[0].name, "IP");
        assert_eq!(caps[0].field.as_deref(), Some("client"));
        assert!(caps[1].grok);
        assert_eq!(caps[1].name, "NUMBER");
        assert_eq!(caps[1].field.as_deref(), Some("bytes"));
        assert_eq!(caps[1].ty, TypeName::Int);
    }

    #[test]
    fn missing_pattern_is_error() {
        let s = parse("format X { coerce a to int; }");
        assert!(matches!(s, Err(SchemaError::MissingPattern(_))));
    }

    /// Fuzz-smoke: the parser must never panic on adversarial/garbage input.
    #[test]
    fn parser_never_panics_on_garbage() {
        let cases = [
            "",
            "{",
            "{{{{",
            "format \u{0}\u{1}\u{2} { pattern: \"",
            "format X { pattern: \"%{",
            "format X { pattern: \"%{NOT_CLOSED\"; }",
            "format X { pattern: \"a\"; } format X { pattern: \"b\"; }",
            &"a".repeat(1 << 16),
            "format \u{7f}\u{fffd} { pattern: \"%{IP:ipv4}\"; coerce ipv4 to enum { A }; }",
            "format X { pattern: \"%{UNKNOWN_PATTERN:field}\"; }",
            "\u{202e}right-to-left\u{202c} %{IP:x}",
        ];
        for c in cases {
            // Must return Ok or Err, never unwind.
            let _ = parse(c);
        }
    }
}
