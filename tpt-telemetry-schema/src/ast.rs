//! Abstract syntax tree for the `.tpt-log` schema DSL.

use std::fmt;

/// A parsed `.tpt-log` schema: a collection of named format blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub formats: Vec<Format>,
}

/// A single `format Name { ... }` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    pub name: String,
    pub pattern: Pattern,
    pub extracts: Vec<Extract>,
    pub coercions: Vec<Coercion>,
    pub redactions: Vec<Redaction>,
}

/// The primary `pattern:` line, decomposed into literal runs and captures.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub parts: Vec<PatternPart>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternPart {
    /// Verbatim text that must appear in the log line.
    Literal(String),
    /// `%{ ... }` capture, either native `%{field:type}` or grok `%{PATTERN:field:type}`.
    Capture(PatternCapture),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternCapture {
    /// Native field name (for `%{field:type}`), or the grok pattern name when grok-flavoured.
    pub name: String,
    /// Resolved scalar type for the capture.
    pub ty: TypeName,
    /// When `true`, `name` is actually a Grok *pattern* (e.g. `NUMBER`, `IP`)
    /// and the capture's output field is `field` (or `name` when unnamed).
    pub grok: bool,
    /// Optional user-supplied output field name for grok captures.
    pub field: Option<String>,
}

/// Scalar coercion target applied to a captured field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeName {
    Int,
    Uint,
    Float,
    Bool,
    String,
    Ipv4,
    Ipv6,
    Ip,
    Mac,
    Timestamp,
}

impl fmt::Display for TypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TypeName::Int => "int",
            TypeName::Uint => "uint",
            TypeName::Float => "float",
            TypeName::Bool => "bool",
            TypeName::String => "string",
            TypeName::Ipv4 => "ipv4",
            TypeName::Ipv6 => "ipv6",
            TypeName::Ip => "ip",
            TypeName::Mac => "mac",
            TypeName::Timestamp => "timestamp",
        };
        f.write_str(s)
    }
}

/// `extract <field> from <source> using regex "..."`.
#[derive(Debug, Clone, PartialEq)]
pub struct Extract {
    pub field: String,
    pub source: String,
    pub regex: String,
}

/// `coerce <field> to <type>` or `coerce <field> to enum { A, B, C }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Coercion {
    pub field: String,
    pub target: CoercionTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoercionTarget {
    Type(TypeName),
    Enum(Vec<String>),
}

/// `redact <field> with hash|mask`.
#[derive(Debug, Clone, PartialEq)]
pub struct Redaction {
    pub field: String,
    pub mode: RedactMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactMode {
    Hash,
    Mask,
}
