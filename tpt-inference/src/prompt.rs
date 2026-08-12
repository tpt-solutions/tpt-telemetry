//! Prompt construction for schema inference.

use crate::error::AttemptContext;

/// System prompt instructing the model to emit a strict `.tpt-log` schema.
pub const SYSTEM_PROMPT: &str = "\
You are a log-format analysis assistant. Given raw log line samples, you produce a \
.tpt-log schema that parses them.

Rules:
- Emit ONLY a `.tpt-log` schema: one or more `format NAME { ... }` blocks.
- Use native captures `%{field:type}` where type is one of: int, uint, float, \
bool, string, ip, ipv4, ipv6, mac, timestamp.
- Use Grok captures `%{PATTERN:field}` (e.g. %{IP:client}) when a standard Grok \
pattern fits.
- Add `coerce field to enum { ... }` for categorical fields, and \
`redact field with mask` for PII (emails, IPs, credit cards).
- Do NOT wrap the schema in markdown code fences. Output the raw schema text only.

Example output:
format CiscoASA {
  pattern: \"%ASA-%{severity:int}-%{msg_id:int}: %{message:string}\";
  coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
}";

/// Build the user prompt from raw samples.
pub fn build_user_prompt(samples: &[&str]) -> String {
    let mut out = String::from(
        "Below are raw log line samples. Infer a .tpt-log schema that matches them.\n\n",
    );
    for (i, s) in samples.iter().enumerate() {
        out.push_str(&format!("--- sample {} ---\n{}\n", i + 1, s.trim_end()));
    }
    out.push_str("\nEmit the .tpt-log schema now:\n");
    out
}

/// Append prior-attempt feedback to the user prompt for the validation loop.
pub fn with_feedback(base: &str, attempts: &[AttemptContext]) -> String {
    if attempts.is_empty() {
        return base.to_string();
    }
    let mut out = base.to_string();
    out.push_str("\n\nPrevious attempts failed to compile. Fix the errors:\n");
    for a in attempts {
        out.push_str(&format!(
            "- attempt {}: {}\n  schema was:\n{}\n",
            a.attempt, a.error, a.schema_text
        ));
    }
    out
}

/// Strip surrounding markdown code fences (``` or ```tpt-log) from a model reply.
pub fn strip_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(stripped) = trimmed
        .strip_prefix("```")
        .and_then(|s| s.strip_suffix("```"))
    {
        // Drop an optional language tag on the opening fence.
        let mut lines = stripped.lines();
        let first = lines.next().unwrap_or("");
        if !first.trim().is_empty() && !first.trim().starts_with('{') {
            return lines.collect::<Vec<_>>().join("\n").trim().to_string();
        }
        return stripped.trim().to_string();
    }
    trimmed.to_string()
}
