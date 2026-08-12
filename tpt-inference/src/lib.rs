//! `tpt-inference` — LLM-assisted `.tpt-log` schema suggestion.
//!
//! Defines the [`InferenceProvider`] trait and a validate-and-retry loop
//! ([`infer_schema`]) that guarantees the suggested schema parses and compiles
//! via the Phase 3 compiler. Ships providers for Claude (Anthropic), OpenAI,
//! OpenRouter, Grok (OpenAI-compatible), and Ollama (local), plus offline mocks.
//!
//! ## Environment
//!
//! Live providers read their bearer token from an environment variable
//! (e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`,
//! `XAI_API_KEY`). When the key is absent, `suggest` returns
//! [`InferenceError::NoApiKey`] rather than panicking.

pub mod clients;
pub mod error;
pub mod prompt;
pub mod provider;

pub use clients::{
    AnthropicProvider, FlakyMockProvider, MockProvider, OllamaProvider, OpenAiCompatProvider,
};
pub use error::{AttemptContext, InferenceError};
pub use prompt::{build_user_prompt, strip_code_fences, SYSTEM_PROMPT};
pub use provider::{infer_schema, InferenceProvider};

/// Convenience: construct the provider selected by `name` with default
/// endpoints/models. Returns an error for unknown names.
pub fn provider_by_name(name: &str) -> Result<Box<dyn InferenceProvider>, InferenceError> {
    let p: Box<dyn InferenceProvider> = match name.to_ascii_lowercase().as_str() {
        "claude" | "anthropic" => Box::new(AnthropicProvider::new()),
        "openai" => Box::new(OpenAiCompatProvider::openai()),
        "openrouter" => Box::new(OpenAiCompatProvider::openrouter()),
        "grok" | "xai" => Box::new(OpenAiCompatProvider::grok()),
        "ollama" => Box::new(OllamaProvider::new()),
        "mock" => Box::new(MockProvider::new(
            "format Sample { pattern: \"%{level:string}: %{message:string}\"; }",
        )),
        other => {
            return Err(InferenceError::Other(format!("unknown provider `{other}`")));
        }
    };
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str =
        "format Sample { pattern: \"%{level:string}: %{message:string}\"; }";
    const INVALID: &str = "this is not a schema at all";

    #[test]
    fn mock_provider_validates() {
        let p = MockProvider::new(VALID);
        let schema = infer_schema(&p, &["INFO: hello", "ERROR: boom"], 2).unwrap();
        assert!(schema.contains("format Sample"));
    }

    #[test]
    fn validation_loop_retries_on_invalid() {
        let p = FlakyMockProvider {
            invalid: INVALID.into(),
            valid: VALID.into(),
        };
        let schema = infer_schema(&p, &["INFO: hello"], 3).unwrap();
        assert!(schema.contains("format Sample"));
    }

    #[test]
    fn validation_loop_fails_after_exhausting_retries() {
        let p = MockProvider::new(INVALID);
        let err = infer_schema(&p, &["INFO: hello"], 2);
        assert!(matches!(err, Err(InferenceError::ValidationFailed(_))));
    }

    #[test]
    fn provider_by_name_unknown() {
        assert!(provider_by_name("nope").is_err());
    }

    #[test]
    fn strip_code_fences_handles_tpt_log_block() {
        let fenced = "```tpt-log\nformat X { pattern: \"%{a:int}\"; }\n```";
        assert_eq!(
            strip_code_fences(fenced),
            "format X { pattern: \"%{a:int}\"; }"
        );
    }
}
