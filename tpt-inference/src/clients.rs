//! Concrete inference providers: Claude (Anthropic), OpenAI, OpenRouter, Grok
//! (OpenAI-compatible), Ollama (local), plus offline mocks for testing.

use crate::error::InferenceError;
use crate::prompt::{build_user_prompt, strip_code_fences, SYSTEM_PROMPT};
use crate::provider::InferenceProvider;
use std::env;

fn api_key(env_var: &str) -> Result<String, InferenceError> {
    let v = env::var(env_var).map_err(|_| InferenceError::NoApiKey(env_var.to_string()))?;
    if v.trim().is_empty() {
        return Err(InferenceError::NoApiKey(env_var.to_string()));
    }
    Ok(v)
}

fn user_message(samples: &[&str], context: &str) -> String {
    let base = build_user_prompt(samples);
    if context.is_empty() {
        base
    } else {
        format!("{base}\n{context}")
    }
}

fn extract_openai_compat(v: &serde_json::Value) -> Result<String, InferenceError> {
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| strip_code_fences(s).to_string())
        .ok_or_else(|| InferenceError::Api("missing choices[0].message.content".into()))
}

// ---------------------------------------------------------------------------
// OpenAI-compatible providers (OpenAI, OpenRouter, Grok, and Ollama's OpenAI
// shim all speak the `/chat/completions` protocol).
// ---------------------------------------------------------------------------

/// A provider for any OpenAI-compatible Chat Completions endpoint.
pub struct OpenAiCompatProvider {
    pub name: String,
    /// Base URL, e.g. `https://api.openai.com/v1`, `https://openrouter.ai/api/v1`,
    /// or `https://api.x.ai/v1` (Grok).
    pub base_url: String,
    pub model: String,
    /// Environment variable holding the bearer token.
    pub api_key_env: String,
}

impl InferenceProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn suggest(&self, samples: &[&str], context: &str) -> Result<String, InferenceError> {
        let key = api_key(&self.api_key_env)?;
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "temperature": 0.0,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_message(samples, context)},
            ],
        });
        let resp = ureq::post(&url)
            .set("Authorization", &format!("Bearer {key}"))
            .set("Content-Type", "application/json")
            .send_json(body)?;
        let v: serde_json::Value = resp.into_json().map_err(|e| InferenceError::Http(e.to_string()))?;
        extract_openai_compat(&v)
    }
}

macro_rules! openai_compat {
    ($( ($ctor:ident, $name:expr, $url:expr, $model:expr, $env:expr) ),* $(,)?) => {
        $(
            impl OpenAiCompatProvider {
                /// Construct the provider with default endpoint/model.
                pub fn $ctor() -> Self {
                    OpenAiCompatProvider {
                        name: $name.into(),
                        base_url: $url.into(),
                        model: $model.into(),
                        api_key_env: $env.into(),
                    }
                }
            }
        )*
    };
}

openai_compat!(
    (openai, "openai", "https://api.openai.com/v1", "gpt-4o-mini", "OPENAI_API_KEY"),
    (openrouter, "openrouter", "https://openrouter.ai/api/v1", "openai/gpt-4o-mini", "OPENROUTER_API_KEY"),
    (grok, "grok", "https://api.x.ai/v1", "grok-beta", "XAI_API_KEY"),
);

// ---------------------------------------------------------------------------
// Anthropic (Claude)
// ---------------------------------------------------------------------------

/// Anthropic Claude provider (Messages API).
pub struct AnthropicProvider {
    pub model: String,
    pub api_key_env: String,
}

impl AnthropicProvider {
    /// Construct with default model/env.
    pub fn new() -> Self {
        AnthropicProvider {
            model: "claude-3-5-sonnet-20241022".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn suggest(&self, samples: &[&str], context: &str) -> Result<String, InferenceError> {
        let key = api_key(&self.api_key_env)?;
        let url = "https://api.anthropic.com/v1/messages";
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": user_message(samples, context)}],
        });
        let resp = ureq::post(url)
            .set("x-api-key", &key)
            .set("anthropic-version", "2023-06-01")
            .set("Content-Type", "application/json")
            .send_json(body)?;
        let v: serde_json::Value = resp.into_json().map_err(|e| InferenceError::Http(e.to_string()))?;
        v["content"][0]["text"]
            .as_str()
            .map(|s| strip_code_fences(s).to_string())
            .ok_or_else(|| InferenceError::Api("missing content[0].text".into()))
    }
}

// ---------------------------------------------------------------------------
// Ollama (local)
// ---------------------------------------------------------------------------

/// Local Ollama provider (`/api/chat`). No API key required.
pub struct OllamaProvider {
    pub model: String,
    pub base_url: String,
}

impl OllamaProvider {
    /// Construct with default local endpoint.
    pub fn new() -> Self {
        OllamaProvider {
            model: "llama3".into(),
            base_url: "http://localhost:11434".into(),
        }
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn suggest(&self, samples: &[&str], context: &str) -> Result<String, InferenceError> {
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_message(samples, context)},
            ],
        });
        let resp = ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)?;
        let v: serde_json::Value = resp.into_json().map_err(|e| InferenceError::Http(e.to_string()))?;
        v["message"]["content"]
            .as_str()
            .map(|s| strip_code_fences(s).to_string())
            .ok_or_else(|| InferenceError::Api("missing message.content".into()))
    }
}

// ---------------------------------------------------------------------------
// Offline mocks (for tests and dry-runs)
// ---------------------------------------------------------------------------

/// Returns a fixed schema string regardless of input. Useful for tests and as a
/// stand-in when no API key is configured.
pub struct MockProvider {
    pub schema: String,
}

impl MockProvider {
    pub fn new(schema: impl Into<String>) -> Self {
        MockProvider {
            schema: schema.into(),
        }
    }
}

impl InferenceProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn suggest(&self, _samples: &[&str], _context: &str) -> Result<String, InferenceError> {
        Ok(self.schema.clone())
    }
}

/// Returns an invalid schema on the first attempt (empty context) and a valid one
/// afterwards, to exercise the validate-and-retry loop.
pub struct FlakyMockProvider {
    pub invalid: String,
    pub valid: String,
}

impl InferenceProvider for FlakyMockProvider {
    fn name(&self) -> &str {
        "flaky-mock"
    }
    fn suggest(&self, _samples: &[&str], context: &str) -> Result<String, InferenceError> {
        if context.is_empty() {
            Ok(self.invalid.clone())
        } else {
            Ok(self.valid.clone())
        }
    }
}
