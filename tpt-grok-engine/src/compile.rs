//! Compilation of Grok pattern strings into `regex::Regex` source.
//!
//! Handles `%{NAME}`, `%{NAME:field}`, and `%{NAME:field:type}` references,
//! recursively expanding named patterns from the standard library
//! (`tpt_telemetry_schema::patterns`).

use crate::error::GrokError;
use regex::escape;
use tpt_telemetry_schema::patterns;

const MAX_DEPTH: usize = 64;

/// A token from a Grok pattern: a literal run, or a `%{ ... }` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Literal(&'a str),
    Capture {
        name: &'a str,
        field: Option<&'a str>,
        ty: Option<&'a str>,
    },
}

/// Tokenize a Grok pattern string into literals and `%{...}` captures.
pub fn tokenize(pattern: &str) -> Result<Vec<Token<'_>>, GrokError> {
    let bytes = pattern.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut lit_start = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if i > lit_start {
                out.push(Token::Literal(&pattern[lit_start..i]));
            }
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
                return Err(GrokError::UnbalancedBrace(pattern.to_string()));
            }
            let content = &pattern[i + 2..j - 1];
            out.push(parse_capture(content)?);
            i = j;
            lit_start = i;
        } else {
            i += 1;
        }
    }
    if lit_start < pattern.len() {
        out.push(Token::Literal(&pattern[lit_start..]));
    }
    Ok(out)
}

fn parse_capture(content: &str) -> Result<Token<'_>, GrokError> {
    let segs: Vec<&str> = content.split(':').collect();
    match segs.as_slice() {
        [name] => Ok(Token::Capture {
            name,
            field: None,
            ty: None,
        }),
        [name, field] => Ok(Token::Capture {
            name,
            field: Some(field),
            ty: None,
        }),
        [name, field, ty] => Ok(Token::Capture {
            name,
            field: Some(field),
            ty: Some(ty),
        }),
        _ => Err(GrokError::BadCapture(content.to_string())),
    }
}

/// Build a `regex` source string from a Grok pattern.
pub fn compile(pattern: &str) -> Result<String, GrokError> {
    let tokens = tokenize(pattern)?;
    let mut out = String::new();
    let mut stack = Vec::new();
    for tok in &tokens {
        match tok {
            Token::Literal(s) => out.push_str(&escape(s)),
            Token::Capture { name, field, .. } => {
                let sub = expand_ref(name, &mut stack)?;
                match field {
                    Some(f) => {
                        if !is_valid_group_name(f) {
                            return Err(GrokError::InvalidGroupName(f.to_string()));
                        }
                        out.push_str(&format!("(?<{}>{})", f, sub));
                    }
                    None => out.push_str(&format!("(?:{})", sub)),
                }
            }
        }
    }
    Ok(out)
}

/// Recursively expand a named pattern reference into its regex fragment.
fn expand_ref(name: &str, stack: &mut Vec<String>) -> Result<String, GrokError> {
    if stack.iter().any(|n| n == name) {
        return Err(GrokError::Cycle(name.to_string()));
    }
    if stack.len() >= MAX_DEPTH {
        return Err(GrokError::TooDeep(name.to_string()));
    }
    stack.push(name.to_string());

    let pat = patterns::lookup(name).ok_or_else(|| GrokError::UnknownPattern(name.to_string()))?;
    let tokens = tokenize(pat.regex)?;
    let mut out = String::new();
    for tok in &tokens {
        match tok {
            // A named pattern's regex definition is already regex syntax; insert
            // it verbatim (do NOT re-escape, which would double-escape metachars).
            Token::Literal(s) => out.push_str(s),
            Token::Capture { name: sub, .. } => {
                let sub_re = expand_ref(sub, stack)?;
                out.push_str(&format!("(?:{})", sub_re));
            }
        }
    }
    stack.pop();
    Ok(out)
}

fn is_valid_group_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Extract the longest mandatory literal run (>= `min_len`) for a fast SIMD
/// pre-scan, or `None` if the pattern has no usable literal anchor.
pub fn fast_literal(pattern: &str, min_len: usize) -> Option<String> {
    let tokens = tokenize(pattern).ok()?;
    let mut best: Option<String> = None;
    for tok in &tokens {
        if let Token::Literal(s) = tok {
            if s.len() >= min_len && best.as_ref().is_none_or(|b| s.len() > b.len()) {
                // Reject literals that are entirely regex-escaped noise.
                if s.chars().any(|c| !matches!(c, '\\' | ' ')) {
                    best = Some(s.to_string());
                }
            }
        }
    }
    best
}
