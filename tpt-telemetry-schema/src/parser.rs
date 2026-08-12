//! Pest-based parser for the `.tpt-log` schema, producing the [`ast::Schema`].

use crate::ast::*;
use crate::patterns;
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashSet;

#[derive(Parser)]
#[grammar = "schema.pest"]
struct SchemaParser;

/// Errors produced while parsing a `.tpt-log` schema.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("duplicate format block: `{0}`")]
    DuplicateFormat(String),
    #[error("duplicate field `{0}` in format `{1}`")]
    DuplicateField(String, String),
    #[error("unknown scalar type `{0}`")]
    UnknownType(String),
    #[error("unknown redaction mode `{0}`")]
    UnknownRedactMode(String),
    #[error("format `{0}` is missing a `pattern:` declaration")]
    MissingPattern(String),
}

/// Parse a `.tpt-log` schema source string into an [`ast::Schema`].
pub fn parse(input: &str) -> Result<Schema, SchemaError> {
    let pairs =
        SchemaParser::parse(Rule::schema, input).map_err(|e| SchemaError::Parse(e.to_string()))?;

    let mut formats = Vec::new();
    let mut seen_formats = HashSet::new();

    for schema_pair in pairs {
        for block in schema_pair.into_inner() {
            match block.as_rule() {
                Rule::format_block => {
                    let mut name = None;
                    let mut pattern = None;
                    let mut extracts = Vec::new();
                    let mut coercions = Vec::new();
                    let mut redactions = Vec::new();
                    let mut seen_fields = HashSet::new();

                    for item in block.into_inner() {
                        match item.as_rule() {
                            Rule::ident => name = Some(item.as_str().to_string()),
                            Rule::pattern_decl => {
                                let string_pair = item.into_inner().next().unwrap();
                                let raw = unquote(string_pair);
                                pattern = Some(tokenize_pattern(&raw)?);
                            }
                            Rule::extract_decl => {
                                let ex = parse_extract(item, name.as_deref().unwrap_or("?"))?;
                                check_field(
                                    &mut seen_fields,
                                    &ex.field,
                                    name.as_deref().unwrap_or("?"),
                                )?;
                                extracts.push(ex);
                            }
                            Rule::coerce_decl => {
                                let co = parse_coerce(item, name.as_deref().unwrap_or("?"))?;
                                check_field(
                                    &mut seen_fields,
                                    &co.field,
                                    name.as_deref().unwrap_or("?"),
                                )?;
                                coercions.push(co);
                            }
                            Rule::redact_decl => {
                                let rd = parse_redact(item)?;
                                redactions.push(rd);
                            }
                            _ => {}
                        }
                    }

                    let name =
                        name.ok_or_else(|| SchemaError::Parse("missing format name".into()))?;
                    if !seen_formats.insert(name.clone()) {
                        return Err(SchemaError::DuplicateFormat(name));
                    }
                    let pattern =
                        pattern.ok_or_else(|| SchemaError::MissingPattern(name.clone()))?;
                    formats.push(Format {
                        name,
                        pattern,
                        extracts,
                        coercions,
                        redactions,
                    });
                }
                Rule::EOI => {}
                _ => {}
            }
        }
    }

    Ok(Schema { formats })
}

fn check_field(seen: &mut HashSet<String>, field: &str, fmt: &str) -> Result<(), SchemaError> {
    if !seen.insert(field.to_string()) {
        return Err(SchemaError::DuplicateField(
            field.to_string(),
            fmt.to_string(),
        ));
    }
    Ok(())
}

fn unquote(pair: pest::iterators::Pair<Rule>) -> String {
    // The `string` rule is a compound atomic (`${ ... }`), so its `as_str()`
    // includes the surrounding quotes and it has no inner pairs.
    let s = pair.as_str();
    s.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(s)
        .to_string()
}

fn parse_extract(pair: pest::iterators::Pair<Rule>, _fmt: &str) -> Result<Extract, SchemaError> {
    let mut it = pair.into_inner();
    let field = it.next().unwrap().as_str().to_string();
    // `from` keyword is implicit; the grammar only yields the two idents + string.
    let source = it.next().unwrap().as_str().to_string();
    let regex = unquote(it.next().unwrap());
    Ok(Extract {
        field,
        source,
        regex,
    })
}

fn parse_coerce(pair: pest::iterators::Pair<Rule>, _fmt: &str) -> Result<Coercion, SchemaError> {
    let mut it = pair.into_inner();
    let field = it.next().unwrap().as_str().to_string();
    let target_pair = it.next().unwrap();
    // `coerce_target` wraps either `enum_decl` or `type_name`; look one level in.
    let inner = target_pair.into_inner().next().unwrap();
    let target = match inner.as_rule() {
        Rule::type_name => CoercionTarget::Type(parse_type(inner.as_str())?),
        Rule::enum_decl => {
            let variants = inner
                .into_inner()
                .map(|v| v.as_str().to_string())
                .collect::<Vec<_>>();
            CoercionTarget::Enum(variants)
        }
        _ => return Err(SchemaError::Parse("invalid coercion target".into())),
    };
    Ok(Coercion { field, target })
}

fn parse_redact(pair: pest::iterators::Pair<Rule>) -> Result<Redaction, SchemaError> {
    let mut it = pair.into_inner();
    let field = it.next().unwrap().as_str().to_string();
    let mode_str = it.next().unwrap().as_str();
    let mode = match mode_str {
        "hash" => RedactMode::Hash,
        "mask" => RedactMode::Mask,
        other => return Err(SchemaError::UnknownRedactMode(other.to_string())),
    };
    Ok(Redaction { field, mode })
}

fn parse_type(s: &str) -> Result<TypeName, SchemaError> {
    match s {
        "int" => Ok(TypeName::Int),
        "uint" => Ok(TypeName::Uint),
        "float" => Ok(TypeName::Float),
        "bool" => Ok(TypeName::Bool),
        "string" => Ok(TypeName::String),
        "ipv4" => Ok(TypeName::Ipv4),
        "ipv6" => Ok(TypeName::Ipv6),
        "ip" => Ok(TypeName::Ip),
        "mac" => Ok(TypeName::Mac),
        "timestamp" => Ok(TypeName::Timestamp),
        other => Err(SchemaError::UnknownType(other.to_string())),
    }
}

/// Split a `pattern:` string into literal runs and `%{ ... }` captures.
fn tokenize_pattern(input: &str) -> Result<Pattern, SchemaError> {
    let mut parts = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut literal_start = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // Flush pending literal.
            if i > literal_start {
                parts.push(PatternPart::Literal(input[literal_start..i].to_string()));
            }
            // Find matching '}'.
            let mut depth = 1usize;
            let mut j = i + 2;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth != 0 {
                return Err(SchemaError::Parse("unbalanced `%{` in pattern".into()));
            }
            let content = &input[i + 2..j - 1];
            parts.push(PatternPart::Capture(parse_capture(content)?));
            i = j;
            literal_start = i;
        } else {
            i += 1;
        }
    }
    if literal_start < input.len() {
        parts.push(PatternPart::Literal(input[literal_start..].to_string()));
    }
    Ok(Pattern { parts })
}

fn parse_capture(content: &str) -> Result<PatternCapture, SchemaError> {
    let segs: Vec<&str> = content.split(':').collect();
    match segs.len() {
        1 => {
            // `%{PATTERN}` — Grok reference, output field named after the pattern.
            Ok(PatternCapture {
                name: segs[0].to_string(),
                ty: TypeName::String,
                grok: true,
                field: None,
            })
        }
        2 => {
            if is_type(segs[1]) {
                // Native `%{field:type}`.
                Ok(PatternCapture {
                    name: segs[0].to_string(),
                    ty: parse_type(segs[1])?,
                    grok: false,
                    field: None,
                })
            } else {
                // Grok `%{PATTERN:field}`.
                Ok(PatternCapture {
                    name: segs[0].to_string(),
                    ty: TypeName::String,
                    grok: true,
                    field: Some(segs[1].to_string()),
                })
            }
        }
        _ => {
            // Grok `%{PATTERN:field:type}`.
            Ok(PatternCapture {
                name: segs[0].to_string(),
                ty: parse_type(segs[2])?,
                grok: true,
                field: Some(segs[1].to_string()),
            })
        }
    }
}

fn is_type(s: &str) -> bool {
    matches!(
        s,
        "int" | "uint" | "float" | "bool" | "string" | "ipv4" | "ipv6" | "ip" | "mac" | "timestamp"
    )
}

/// Whether a capture references a known standard Grok pattern name.
pub fn is_standard_grok(name: &str) -> bool {
    patterns::lookup(name).is_some()
}
