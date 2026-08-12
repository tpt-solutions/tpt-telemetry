//! Compilation of a `.tpt-log` [`Schema`] AST into a zero-copy runtime parser.
//!
//! The compiler lowers each `format { ... }` block into a [`CompiledFormat`]
//! consisting of a flat list of [`Seg`]ments (literals and typed captures). At
//! run time, [`CompiledFormat::parse`] matches a log line against those segments
//! by *reference*: every captured field value is a `&str` slice borrowed from the
//! input line, and numeric/timestamp/enum coercions are parsed in place without
//! heap allocation. The only heap use in the steady-state loop is the small,
//! caller-reused `Vec` of fields (see [`MatchCtx`]).
//!
//! Grok captures (`%{PATTERN:field}` / `%{PATTERN:field:type}`) are mapped to
//! native scalar types where possible (e.g. `%{IP:src}` → `TypeName::Ip`). Names
//! without a native mapping are rejected at compile time so the matcher can stay
//! allocation-free; fully-arbitrary Grok patterns remain available via
//! `tpt-grok-engine`.

use crate::error::{CompileError, Result};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use tpt_telemetry_schema::ast::{CoercionTarget, PatternPart, RedactMode, Schema, TypeName};

/// A single compiled pattern segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    /// Verbatim text that must appear at this position in the line.
    Literal(String),
    /// A typed capture, borrowing its value from the input at run time.
    Capture { field: String, ty: TypeName },
}

/// A compiled `format` block: segments plus post-match coercions and redactions.
#[derive(Debug, Clone)]
pub struct CompiledFormat {
    pub name: String,
    pub segments: Vec<Seg>,
    pub coercions: Vec<(String, CoercionTarget)>,
    pub redactions: Vec<(String, RedactMode)>,
}

impl CompiledFormat {
    /// Construct a compiled format (used by the code generator and tests).
    pub fn new(
        name: String,
        segments: Vec<Seg>,
        coercions: Vec<(String, CoercionTarget)>,
        redactions: Vec<(String, RedactMode)>,
    ) -> Self {
        CompiledFormat {
            name,
            segments,
            coercions,
            redactions,
        }
    }

    /// Match and fully type a log line, applying coercions and redactions.
    pub fn parse<'a>(&'a self, line: &'a str) -> Option<Record<'a>> {
        let raw = self.match_raw(line)?;
        Some(build_record(self, raw))
    }

    /// Match a line, returning borrowed `(field, value)` pairs with no coercion.
    /// This is the zero-allocation fast path: it allocates nothing beyond the
    /// reused buffers in [`MatchCtx`].
    pub fn match_raw<'a>(&'a self, line: &'a str) -> Option<RawMatch<'a>> {
        let mut ctx = MatchCtx::new(self.segments.len());
        if self.match_into(line, &mut ctx) {
            Some(ctx.into_raw(line, &self.name, &self.segments))
        } else {
            None
        }
    }

    /// Match into a reused [`MatchCtx`] so the steady-state loop performs no
    /// allocation after the first iteration.
    pub fn match_into(&self, line: &str, ctx: &mut MatchCtx) -> bool {
        ctx.reset();
        match_segments(&self.segments, line, 0, 0, ctx)
    }
}

/// A compiled schema: all formats, ready to dispatch a line to the right one.
#[derive(Debug, Clone)]
pub struct CompiledSchema {
    pub formats: Vec<CompiledFormat>,
}

impl CompiledSchema {
    /// Construct a compiled schema.
    pub fn new(formats: Vec<CompiledFormat>) -> Self {
        CompiledSchema { formats }
    }

    /// Compile a parsed [`Schema`] into a [`CompiledSchema`].
    pub fn compile(schema: &Schema) -> Result<CompiledSchema> {
        let mut formats = Vec::with_capacity(schema.formats.len());
        for f in &schema.formats {
            let mut segments = Vec::new();
            for part in &f.pattern.parts {
                match part {
                    PatternPart::Literal(s) => segments.push(Seg::Literal(s.clone())),
                    PatternPart::Capture(c) => {
                        let (field, ty) = if c.grok {
                            let field = c.field.clone().unwrap_or_else(|| c.name.clone());
                            let ty = if c.ty != TypeName::String {
                                c.ty
                            } else {
                                grok_native_type(&c.name)
                                    .ok_or_else(|| CompileError::UnsupportedGrok(c.name.clone()))?
                            };
                            (field, ty)
                        } else {
                            (c.name.clone(), c.ty)
                        };
                        segments.push(Seg::Capture { field, ty });
                    }
                }
            }

            let coercions: Vec<(String, CoercionTarget)> = f
                .coercions
                .iter()
                .map(|c| (c.field.clone(), c.target.clone()))
                .collect();
            let redactions: Vec<(String, RedactMode)> = f
                .redactions
                .iter()
                .map(|r| (r.field.clone(), r.mode))
                .collect();

            formats.push(CompiledFormat::new(
                f.name.clone(),
                segments,
                coercions,
                redactions,
            ));
        }
        Ok(CompiledSchema { formats })
    }

    /// Parse a single line, returning the first format that matches.
    pub fn parse_line<'a>(&'a self, line: &'a str) -> Option<Record<'a>> {
        for fmt in &self.formats {
            if let Some(rec) = fmt.parse(line) {
                return Some(rec);
            }
        }
        None
    }

    /// Match a line into a reused context, returning the matching format name
    /// and borrowed fields (no coercions applied).
    pub fn match_line<'a>(&'a self, line: &'a str, ctx: &mut MatchCtx) -> Option<RawMatch<'a>> {
        for fmt in &self.formats {
            ctx.reset();
            if fmt.match_into(line, ctx) {
                return Some(ctx.into_raw(line, &fmt.name, &fmt.segments));
            }
        }
        None
    }

    /// Zero-allocation match test: does any format match this line? Reuses `ctx`
    /// so the steady-state loop performs no heap allocation.
    pub fn matches(&self, line: &str, ctx: &mut MatchCtx) -> bool {
        for fmt in &self.formats {
            ctx.reset();
            if fmt.match_into(line, ctx) {
                return true;
            }
        }
        false
    }
}

/// Map a standard Grok pattern name to a native scalar type so it can be matched
/// without `regex` allocation.
pub(crate) fn grok_native_type(name: &str) -> Option<TypeName> {
    Some(match name {
        "INT" | "NUMBER" | "BASE10NUM" => TypeName::Int,
        "POSINT" | "NONNEGINT" | "PORT" | "BASE16NUM" => TypeName::Uint,
        "FLOAT" | "BASE16FLOAT" => TypeName::Float,
        "IPV4" => TypeName::Ipv4,
        "IPV6" => TypeName::Ipv6,
        "IP" => TypeName::Ip,
        "MAC" => TypeName::Mac,
        "WORD" | "NOTSPACE" | "DATA" | "GREEDYDATA" | "QS" | "HOSTNAME" | "USERNAME" | "USER"
        | "EMAILADDRESS" | "SPACE" => TypeName::String,
        "MONTH" | "MONTHDAY" | "YEAR" | "HOUR" | "MINUTE" | "SECOND" | "TIME" | "MONTHNUM"
        | "MONTHNUM2" | "ISO8601_SECOND" => TypeName::Timestamp,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Zero-copy matching
// ---------------------------------------------------------------------------

/// Reusable scratch buffers for the matcher. Clearing and re-filling these does
/// not reallocate after the first capacity, keeping the steady-state loop
/// allocation-free.
pub struct MatchCtx {
    /// Captured `(segment_index, start, end)` for each capture segment, in order.
    caps: Vec<(usize, usize, usize)>,
}

impl MatchCtx {
    /// Create a context sized for `n_segments` captures.
    pub fn new(n_segments: usize) -> Self {
        MatchCtx {
            caps: Vec::with_capacity(n_segments),
        }
    }

    fn reset(&mut self) {
        self.caps.clear();
    }

    /// Build a [`RawMatch`] from the captured ranges, borrowing values from
    /// `line` and field names from `segs`.
    pub fn into_raw<'a>(&self, line: &'a str, name: &'a str, segs: &'a [Seg]) -> RawMatch<'a> {
        let mut fields = Vec::with_capacity(self.caps.len());
        for (si, start, end) in &self.caps {
            if let Seg::Capture { field, .. } = &segs[*si] {
                fields.push(Field {
                    name: field.as_str(),
                    value: &line[*start..*end],
                });
            }
        }
        RawMatch {
            format: name,
            fields,
        }
    }
}

/// A matched line before coercion: borrowed `(field, value)` pairs.
#[derive(Debug, Clone)]
pub struct RawMatch<'a> {
    pub format: &'a str,
    pub fields: Vec<Field<'a>>,
}

/// A single borrowed field capture.
#[derive(Debug, Clone)]
pub struct Field<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

/// Recursively match `segs` against `line` starting at `pos`, consuming segments
/// from `si` onward. Pushes capture ranges into `ctx.caps`. Returns `true` on a
/// full match (consuming the entire line).
fn match_segments(segs: &[Seg], line: &str, si: usize, pos: usize, ctx: &mut MatchCtx) -> bool {
    if si == segs.len() {
        return pos == line.len();
    }
    match &segs[si] {
        Seg::Literal(l) => {
            if line[pos..].starts_with(l.as_str()) {
                return match_segments(segs, line, si + 1, pos + l.len(), ctx);
            }
            false
        }
        Seg::Capture { ty, .. } => {
            // Greedy: try the longest possible capture first, shrinking until the
            // remainder of the pattern can match.
            let mut end = line.len() + 1;
            while end > pos {
                end -= 1;
                let sub = &line[pos..end];
                if !type_matches(*ty, sub) {
                    continue;
                }
                ctx.caps.push((si, pos, end));
                if match_segments(segs, line, si + 1, end, ctx) {
                    return true;
                }
                ctx.caps.pop();
            }
            // Allow zero-length captures for nullable string segments.
            if *ty == TypeName::String && pos <= line.len() {
                ctx.caps.push((si, pos, pos));
                if match_segments(segs, line, si + 1, pos, ctx) {
                    return true;
                }
                ctx.caps.pop();
            }
            false
        }
    }
}

/// Does `sub` satisfy the scalar type `ty` (without committing to a parsed value)?
fn type_matches(ty: TypeName, sub: &str) -> bool {
    if sub.is_empty() {
        return false;
    }
    match ty {
        TypeName::Int => sub.parse::<i64>().is_ok(),
        TypeName::Uint => sub.parse::<u64>().is_ok(),
        TypeName::Float => sub.parse::<f64>().is_ok(),
        TypeName::Bool => {
            matches!(
                sub.to_ascii_lowercase().as_str(),
                "true" | "false" | "1" | "0"
            )
        }
        TypeName::String => true,
        TypeName::Ipv4 => sub.parse::<Ipv4Addr>().is_ok(),
        TypeName::Ipv6 => sub.parse::<Ipv6Addr>().is_ok(),
        TypeName::Ip => sub.parse::<IpAddr>().is_ok(),
        TypeName::Mac => parse_mac(sub).is_some(),
        TypeName::Timestamp => parse_timestamp(sub).is_some(),
    }
}

/// Parse an `xx:xx:xx:xx:xx:xx` / `xx-xx-xx-xx-xx-xx` MAC address.
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let sep = if s.contains(':') { b':' } else { b'-' };
    let mut out = [0u8; 6];
    let mut i = 0;
    for part in s.split(|c| c as u8 == sep) {
        if i >= 6 || part.len() != 2 {
            return None;
        }
        out[i] = u8::from_str_radix(part, 16).ok()?;
        i += 1;
    }
    if i == 6 {
        Some(out)
    } else {
        None
    }
}

/// Parse a timestamp into `(unix_seconds, nanoseconds)`.
fn parse_timestamp(s: &str) -> Option<(i64, u32)> {
    use chrono::DateTime;
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some((dt.timestamp(), dt.timestamp_subsec_nanos()));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some((
                dt.and_utc().timestamp(),
                dt.and_utc().timestamp_subsec_nanos(),
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Typed records (post-coercion / redaction)
// ---------------------------------------------------------------------------

/// A strongly-typed field value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value<'a> {
    Str(&'a str),
    OwnedString(String),
    Int(i64),
    Uint(u64),
    Float(f64),
    Bool(bool),
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    Ip(IpAddr),
    Mac([u8; 6]),
    Timestamp(i64),
    Enum(u8),
}

/// A fully-typed, optionally-redacted record produced from a matched line.
#[derive(Debug, Clone)]
pub struct Record<'a> {
    pub format: &'a str,
    pub fields: Vec<TypedField<'a>>,
}

/// A single typed field on a [`Record`].
#[derive(Debug, Clone)]
pub struct TypedField<'a> {
    pub name: &'a str,
    pub value: Value<'a>,
}

/// Build a [`Record`] from a matched line, applying coercions and redactions.
fn build_record<'a>(fmt: &CompiledFormat, raw: RawMatch<'a>) -> Record<'a> {
    // Map each captured field to its declared (segment) type.
    let mut declared: std::collections::HashMap<&str, TypeName> = std::collections::HashMap::new();
    for seg in &fmt.segments {
        if let Seg::Capture { field, ty } = seg {
            declared.insert(field.as_str(), *ty);
        }
    }

    let mut fields: Vec<TypedField<'a>> = raw
        .fields
        .iter()
        .map(|f| {
            let ty = declared.get(f.name).copied().unwrap_or(TypeName::String);
            let value = coerce_field(fmt, f.name, ty, f.value);
            TypedField {
                name: f.name,
                value,
            }
        })
        .collect();

    // Apply redactions over the *original* captured text (not the coerced value,
    // which for numeric/IP types has no string form).
    for (field, mode) in &fmt.redactions {
        if let Some(tf) = fields.iter_mut().find(|tf| tf.name == field.as_str()) {
            let raw_text = raw
                .fields
                .iter()
                .find(|f| f.name == field.as_str())
                .map(|f| f.value)
                .unwrap_or("");
            tf.value = match mode {
                RedactMode::Mask => Value::OwnedString(mask_value(raw_text)),
                RedactMode::Hash => Value::OwnedString(hash_value(raw_text)),
            };
        }
    }

    Record {
        format: raw.format,
        fields,
    }
}

impl<'a> Value<'a> {
    /// Best-effort string view of the value (used by redaction and OTLP export).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::OwnedString(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Apply a coercion (enum or type) to a captured value, falling back to the
/// capture's declared type when no coercion targets the field.
fn coerce_field<'a>(
    fmt: &CompiledFormat,
    name: &str,
    declared: TypeName,
    raw: &'a str,
) -> Value<'a> {
    for (field, target) in &fmt.coercions {
        if field == name {
            return match target {
                CoercionTarget::Type(t) => coerce_scalar(*t, raw),
                CoercionTarget::Enum(variants) => {
                    // Numeric severity codes index directly into the variant list.
                    if let Ok(n) = raw.parse::<usize>() {
                        if n < variants.len() {
                            return Value::Enum(n as u8);
                        }
                    }
                    match variants.iter().position(|v| v.eq_ignore_ascii_case(raw)) {
                        Some(idx) => Value::Enum(idx as u8),
                        None => Value::Str(raw),
                    }
                }
            };
        }
    }
    // No explicit coercion: coerce to the capture's declared type.
    coerce_scalar(declared, raw)
}

/// Coerce a raw slice to a specific scalar type.
fn coerce_scalar(ty: TypeName, raw: &str) -> Value<'_> {
    match ty {
        TypeName::Int => raw.parse().map(Value::Int).unwrap_or(Value::Str(raw)),
        TypeName::Uint => raw.parse().map(Value::Uint).unwrap_or(Value::Str(raw)),
        TypeName::Float => raw.parse().map(Value::Float).unwrap_or(Value::Str(raw)),
        TypeName::Bool => Value::Bool(matches!(
            raw.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )),
        TypeName::String => Value::Str(raw),
        TypeName::Ipv4 => raw
            .parse::<Ipv4Addr>()
            .map(Value::Ipv4)
            .unwrap_or(Value::Str(raw)),
        TypeName::Ipv6 => raw
            .parse::<Ipv6Addr>()
            .map(Value::Ipv6)
            .unwrap_or(Value::Str(raw)),
        TypeName::Ip => raw
            .parse::<IpAddr>()
            .map(Value::Ip)
            .unwrap_or(Value::Str(raw)),
        TypeName::Mac => parse_mac(raw).map(Value::Mac).unwrap_or(Value::Str(raw)),
        TypeName::Timestamp => parse_timestamp(raw)
            .map(|(s, _)| Value::Timestamp(s))
            .unwrap_or(Value::Str(raw)),
    }
}

/// Mask a value, preserving the last two characters (e.g. `192.168.1.1` → `*****1`).
fn mask_value(s: &str) -> String {
    let bytes = s.as_bytes();
    let keep = bytes.len().min(2);
    let masked = bytes.len().saturating_sub(keep);
    let mut out = String::with_capacity(s.len());
    out.extend(std::iter::repeat_n('*', masked));
    out.push_str(&s[s.len() - keep..]);
    out
}

/// Deterministically hash a value to a fixed-length hex digest (FNV-1a).
fn hash_value(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_telemetry_schema::parse;

    const ASA: &str = r#"
        format CiscoASA {
          pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
          coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
          redact message with mask;
        }
    "#;

    #[test]
    fn compiles_and_matches() {
        let schema = parse(ASA).unwrap();
        let cs = CompiledSchema::compile(&schema).unwrap();
        let line = "%ASA-3-106001: connection denied from 10.0.0.5";
        let rec = cs.parse_line(line).unwrap();
        assert_eq!(rec.format, "CiscoASA");
        let sev = rec.fields.iter().find(|f| f.name == "severity").unwrap();
        assert_eq!(sev.value, Value::Enum(3)); // WARNING index 3
        let msg = rec.fields.iter().find(|f| f.name == "message").unwrap();
        assert!(matches!(msg.value, Value::OwnedString(_)));
        assert!(msg.value.as_str().unwrap().starts_with("*****"));
    }

    #[test]
    fn grok_ip_maps_natively() {
        let schema = parse(r#"format G { pattern: "%{IP:client} %{WORD:action}"; }"#).unwrap();
        let cs = CompiledSchema::compile(&schema).unwrap();
        let rec = cs.parse_line("10.0.0.5 accepted").unwrap();
        let client = rec.fields.iter().find(|f| f.name == "client").unwrap();
        assert_eq!(client.value, Value::Ip("10.0.0.5".parse().unwrap()));
    }

    #[test]
    fn unknown_grok_is_rejected() {
        let schema = parse(r#"format G { pattern: "%{NOTREAL:x}"; }"#).unwrap();
        assert!(CompiledSchema::compile(&schema).is_err());
    }
}
