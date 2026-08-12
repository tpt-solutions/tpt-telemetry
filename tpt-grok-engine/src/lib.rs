//! `tpt-grok-engine` — a SIMD-accelerated Grok pattern matcher.
//!
//! Compiles Grok pattern strings (with `%{NAME:field}` references from the
//! standard/ECS library) into `regex::Regex` source, and matches them against
//! log lines. A two-stage hot path uses `memchr`'s vectorized substring search
//! to reject non-matching lines before running the full regex.

pub mod compile;
pub mod error;

pub use error::GrokError;

use compile::{compile, fast_literal, tokenize};
use memchr::memmem;
use regex::Regex;

/// A compiled Grok pattern.
pub struct Grok {
    inner: Regex,
    /// Longest mandatory literal run, used for the fast SIMD pre-scan.
    fast_literal: Option<String>,
}

impl Grok {
    /// Compile a Grok pattern string.
    pub fn new(pattern: &str) -> Result<Grok, GrokError> {
        let re_src = compile(pattern)?;
        let inner = Regex::new(&re_src)?;
        let fast_literal = fast_literal(pattern, 3);
        Ok(Grok {
            inner,
            fast_literal,
        })
    }

    /// The compiled regex source (useful for debugging / golden files).
    pub fn regex_source(&self) -> &str {
        self.inner.as_str()
    }

    /// All capture group names declared by this pattern.
    pub fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        self.inner.capture_names()
    }

    /// Baseline match: run the full regex anywhere in `input`.
    pub fn find<'a>(&self, input: &'a str) -> Option<Match<'a>> {
        let caps = self.inner.captures(input)?;
        let names = self
            .inner
            .capture_names()
            .map(|n| n.map(str::to_owned))
            .collect();
        Some(Match { caps, names })
    }

    /// SIMD-accelerated hot path: reject lines missing the fast literal, then
    /// fall back to the full regex. Returns `None` immediately when the
    /// mandatory literal anchor is absent.
    pub fn scan<'a>(&self, input: &'a str) -> Option<Match<'a>> {
        if let Some(needle) = &self.fast_literal {
            memmem::find(input.as_bytes(), needle.as_bytes())?;
        }
        self.find(input)
    }
}

/// A successful Grok match, exposing named capture groups.
pub struct Match<'a> {
    caps: regex::Captures<'a>,
    names: Vec<Option<String>>,
}

impl<'a> Match<'a> {
    /// The full matched text.
    pub fn as_str(&self) -> &'a str {
        self.caps.get(0).map_or("", |m| m.as_str())
    }

    /// Value of a named capture group, if present.
    pub fn get(&self, name: &str) -> Option<&'a str> {
        self.caps.name(name).map(|m| m.as_str())
    }

    /// Range of a named capture group, if present.
    pub fn range(&self, name: &str) -> Option<std::ops::Range<usize>> {
        self.caps.name(name).map(|m| m.range())
    }

    /// Iterate `(name, value)` for all named groups that matched.
    pub fn named(&self) -> impl Iterator<Item = (&str, &str)> {
        self.caps.iter().enumerate().filter_map(move |(i, m)| {
            let m = m?;
            let name = self.names.get(i).and_then(|n| n.as_deref())?;
            Some((name, m.as_str()))
        })
    }
}

/// Convenience: tokenize a pattern (re-exported for tooling/tests).
pub fn tokenize_pattern(pattern: &str) -> Result<Vec<compile::Token<'_>>, GrokError> {
    tokenize(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ip_with_field() {
        let g = Grok::new("%{IP:client} %{WORD:action}").unwrap();
        let m = g.find("192.168.1.1 accepted").unwrap();
        assert_eq!(m.get("client"), Some("192.168.1.1"));
        assert_eq!(m.get("action"), Some("accepted"));
    }

    #[test]
    fn number_coercion_field() {
        let g = Grok::new("%{NUMBER:bytes:int}").unwrap();
        let m = g.find("bytes=10423").unwrap();
        assert_eq!(m.get("bytes"), Some("10423"));
    }

    #[test]
    fn nested_pattern_expansion() {
        // NUMBER -> BASE10NUM, IP -> IPV4|IPV6, all expanded.
        let g = Grok::new("%{IP:src} %{NUMBER:port}").unwrap();
        assert!(g.regex_source().contains("25[0-5]")); // IPV4 expansion present
    }

    #[test]
    fn simd_scan_rejects_missing_literal() {
        let g = Grok::new("%ASA-%{INT:severity}-%{NUMBER:msg_id}: %{GREEDYDATA:message}").unwrap();
        // The fast literal "%ASA-" must be present.
        assert!(g.scan("random line here").is_none());
        let m = g.scan("%ASA-3-106001: connection denied").unwrap();
        assert_eq!(m.get("severity"), Some("3"));
        assert_eq!(m.get("msg_id"), Some("106001"));
    }

    #[test]
    fn unknown_pattern_errors() {
        assert!(Grok::new("%{NOT_A_REAL_PATTERN:x}").is_err());
    }

    #[test]
    fn scan_matches_baseline_on_hits() {
        let pat = "%{IP:client} requested %{DATA:path}";
        let g = Grok::new(pat).unwrap();
        let line = "10.0.0.5 requested /index.html";
        assert_eq!(
            g.scan(line).map(|m| m.get("client")),
            g.find(line).map(|m| m.get("client"))
        );
    }
}
