//! Curated standard Grok pattern definitions.
//!
//! These mirror the base Logstash/Grok pattern library (and the ECS-oriented
//! subset), so `.tpt-log` schemas can reference `%{PATTERN:field}` captures
//! exactly as they would in a Logstash pipeline. Patterns may themselves
//! reference other patterns via `%{NAME}`; the grok engine expands those
//! recursively at compile time.

/// A single named Grok pattern: its name and the regex it expands to.
#[derive(Debug, Clone, Copy)]
pub struct GrokPattern {
    pub name: &'static str,
    pub regex: &'static str,
    /// `true` for patterns whose expansion is anchored/zero-width-safe and may
    /// appear unanchored inside a larger expression.
    pub safe_unanchored: bool,
}

/// The canonical base Grok pattern library.
pub const STANDARD_PATTERNS: &[GrokPattern] = &[
    GrokPattern {
        name: "BASE10NUM",
        regex: r"(?:[+-]?(?:(?:[0-9]+(?:\.[0-9]+)?)|(?:\.[0-9]+)))",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "BASE16NUM",
        regex: r"(?:[+-]?(?:0x)?(?:[0-9A-Fa-f]+))",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "BASE16FLOAT",
        regex: r"(?:[+-]?(?:0x)?(?:(?:[0-9A-Fa-f]+(?:\.[0-9A-Fa-f]*)?)|(?:\.[0-9A-Fa-f]+)))",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "NUMBER",
        regex: r"(?:%{BASE10NUM})",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "POSINT",
        regex: r"(?:[1-9][0-9]*)",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "NONNEGINT",
        regex: r"(?:[0-9]+)",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "INT",
        regex: r"(?:[+-]?(?:[0-9]+))",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "WORD",
        regex: r"\b\w+\b",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "NOTSPACE",
        regex: r"\S+",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "SPACE",
        regex: r"\s*",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "DATA",
        regex: r".*?",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "GREEDYDATA",
        regex: r".*",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "QS",
        regex: r"(?:[\x22][^\x22]*[\x22]|[\x22][\x22]|[^\x22]*)",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "IPV4",
        regex: r"(?:(?:25[0-5]|2[0-4][0-9]|[0-1]?[0-9]{1,2})[.](?:25[0-5]|2[0-4][0-9]|[0-1]?[0-9]{1,2})[.](?:25[0-5]|2[0-4][0-9]|[0-1]?[0-9]{1,2})[.](?:25[0-5]|2[0-4][0-9]|[0-1]?[0-9]{1,2}))",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "IPV6",
        regex: r"(?:[::fF]{1,4}:){1,7}(?:[0-9a-fA-F]{1,4}|:)|(?:[0-9a-fA-F]{1,4}:){1,7}:|(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|(?:[0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|(?:[0-9a-fA-F]{1,4}:){1,5}(?::[0-9a-fA-F]{1,4}){1,2}|(?:[0-9a-fA-F]{1,4}:){1,4}(?::[0-9a-fA-F]{1,4}){1,3}|(?:[0-9a-fA-F]{1,4}:){1,3}(?::[0-9a-fA-F]{1,4}){1,4}|(?:[0-9a-fA-F]{1,4}:){1,2}(?::[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:(?:(?::[0-9a-fA-F]{1,4}){1,6})|:(?:(?::[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(?::[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(?:ffff(?::0{1,4}){0,1}:){0,1}(?:(?:25[0-5]|(?:2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(?:25[0-5]|(?:2[0-4]|1{0,1}[0-9]){0,1}[0-9])|(?:[0-9a-fA-F]{1,4}:){1,4}:(?:(?:25[0-5]|(?:2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(?:25[0-5]|(?:2[0-4]|1{0,1}[0-9]){0,1}[0-9])",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "IP",
        regex: r"(?:%{IPV6}|%{IPV4})",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "MAC",
        regex: r"(?:(?:[0-9A-Fa-f]{2}(?::|-)){5}[0-9A-Fa-f]{2})",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "EMAILADDRESS",
        regex: r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",
        safe_unanchored: true,
    },
    // ECS-oriented timestamp patterns (common syslog/RFC3339 forms).
    GrokPattern {
        name: "MONTH",
        regex: r"\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\b",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "MONTHDAY",
        regex: r"(?:0[1-9]|[12][0-9]|3[01]|[1-9])",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "YEAR",
        regex: r"(?:\d\d){1,2}",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "HOUR",
        regex: r"(?:2[0123]|[01]?[0-9])",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "MINUTE",
        regex: r"(?:[0-5][0-9])",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "SECOND",
        regex: r"(?:(?:[0-5]?[0-9]|60)(?:[.,][0-9]+)?)",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "TIME",
        regex: r"(?:2[0123]|[01]?[0-9]):(?:[0-5][0-9])(?::(?:(?:[0-5]?[0-9]|60)(?:[.,][0-9]+)?))?",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "MONTHNUM",
        regex: r"(?:0?[1-9]|1[0-2])",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "MONTHNUM2",
        regex: r"(?:0[1-9]|1[0-2])",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "ISO8601_SECOND",
        regex: r"(?:\d\d){1,2}-(?:0?[1-9]|1[0-2])-(?:0[1-9]|[12][0-9]|3[01]|[1-9])[T ](?:2[0123]|[01]?[0-9]):?(?:[0-5][0-9])(?::?(?:(?:[0-5]?[0-9]|60)(?:[.,][0-9]+)?))?",
        safe_unanchored: true,
    },
    // Common protocol / host patterns.
    GrokPattern {
        name: "HOSTNAME",
        regex: r"\b(?:[0-9A-Za-z][0-9A-Za-z-]{0,62})(?:\.[0-9A-Za-z][0-9A-Za-z-]{0,62})*\b",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "PORT",
        regex: r"(?:[0-9]+)",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "USERNAME",
        regex: r"[a-zA-Z0-9._-]+",
        safe_unanchored: true,
    },
    GrokPattern {
        name: "USER",
        regex: r"%{USERNAME}",
        safe_unanchored: true,
    },
];

/// Look up a standard Grok pattern by name.
pub fn lookup(name: &str) -> Option<&'static GrokPattern> {
    STANDARD_PATTERNS.iter().find(|p| p.name == name)
}

/// All standard pattern names (used for `%{NAME}` resolution and validation).
pub fn known_names() -> impl Iterator<Item = &'static str> {
    STANDARD_PATTERNS.iter().map(|p| p.name)
}
