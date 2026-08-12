//! Security & safety helpers: PII redaction (see [`Value`] redaction in
//! `compile`) and log-injection prevention for downstream SIEM rendering.

/// Characters that must never survive into a rendered SIEM query/field value.
///
/// We strip C0/C1 control characters (including NUL, CR, LF, and the ANSI
/// separator families) so an attacker cannot break out of a field, inject a
/// newline into a syslog stream, or smuggle a second query past the renderer.
fn is_injection_byte(b: u8) -> bool {
    b < 0x20 || b == 0x7f || (0x80..=0x9f).contains(&b)
}

/// Sanitize an extracted field value for safe rendering downstream (SIEM query,
/// CSV, JSON string, etc.). Control bytes are removed; all other bytes are kept.
///
/// This is the "sanitize before render" guard required by the design spec: a
/// field extracted from untrusted log text must not be able to inject a
/// delimiter, newline, or second record into the rendered output.
pub fn sanitize(value: &str) -> std::borrow::Cow<'_, str> {
    if value.bytes().any(is_injection_byte) {
        let cleaned: String = value
            .bytes()
            .filter(|&b| !is_injection_byte(b))
            .map(char::from)
            .collect();
        std::borrow::Cow::Owned(cleaned)
    } else {
        std::borrow::Cow::Borrowed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_control_bytes() {
        // Embedded NUL + CR + LF must be removed.
        let dirty = "user\x00admin\r\n injected";
        assert_eq!(sanitize(dirty), "useradmin injected");
    }

    #[test]
    fn pass_through_clean_values() {
        let clean = "10.0.0.5 accepted login";
        assert!(matches!(sanitize(clean), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn redaction_masks_pii() {
        use crate::compile::{CompiledSchema, Value};
        use tpt_telemetry_schema::parse;
        let schema = parse(
            r#"format Auth { pattern: "%{IP:client} logged in"; redact client with mask; }"#,
        )
        .unwrap();
        let cs = CompiledSchema::compile(&schema).unwrap();
        let rec = cs.parse_line("10.0.0.5 logged in").unwrap();
        let client = rec.fields.iter().find(|f| f.name == "client").unwrap();
        match &client.value {
            Value::OwnedString(s) => {
                assert!(s.starts_with("***"));
                assert_eq!(s.len(), "10.0.0.5".len());
                assert_ne!(s, "10.0.0.5");
            }
            other => panic!("expected masked string, got {other:?}"),
        }
    }
}
