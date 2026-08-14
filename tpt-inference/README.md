# tpt-inference

LLM-assisted `.tpt-log` schema suggestion for
[`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Defines the `InferenceProvider` trait and a validate-and-retry loop
(`infer_schema`) that guarantees the suggested schema parses and compiles via the
compiler. Ships providers for Claude (Anthropic), OpenAI, OpenRouter, Grok
(OpenAI-compatible), and Ollama (local), plus offline mocks.

A companion binary (`tpt-inference`) wraps the library for CLI use.

## Features

- **Provider trait** — `InferenceProvider` abstracts any LLM backend; implement it
  for your own endpoint.
- **Built-in providers** — Anthropic (Claude), OpenAI, OpenRouter, Grok/X.AI
  (OpenAI-compatible), and Ollama (local).
- **Validate-and-retry** — `infer_schema` loops: each suggestion is parsed and
  compiled; invalid output is fed back to the model up to `max_retries` times.
- **Offline mocks** — `MockProvider` / `FlakyMockProvider` for tests and demos.
- **Provider selection** — `provider_by_name("claude" | "openai" | "openrouter" |
  "grok" | "ollama" | "mock")` constructs the default provider.
- **No-key safety** — providers read their bearer token from an environment
  variable and return `InferenceError::NoApiKey` when it is absent (no panic).

## Installation

```toml
[dependencies]
tpt-inference = "0.1.0"
```

## Usage

### Suggesting a schema from samples

```rust
use tpt_inference::{infer_schema, provider_by_name};

let provider = provider_by_name("claude").expect("known provider");
let samples = [
    "INFO: user logged in",
    "ERROR: disk full",
];
let schema = infer_schema(&*provider, &samples, 3).expect("valid schema");
println!("{schema}");
```

### Writing your own provider

```rust
use tpt_inference::{InferenceProvider, AttemptContext};

struct MyProvider;

impl InferenceProvider for MyProvider {
    fn name(&self) -> &str { "my-provider" }
    fn suggest(&self, prompt: &str, ctx: &AttemptContext) -> Result<String, tpt_inference::InferenceError> {
        // call your LLM, returning raw .tpt-log text (code fences are stripped)
        todo!()
    }
}
```

### Environment variables

| Provider | Env var |
|----------|---------|
| Anthropic | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |
| Grok / X.AI | `XAI_API_KEY` |
| Ollama | (local; no key) |

## CLI

```bash
export ANTHROPIC_API_KEY=...
cargo run -p tpt-inference -- --provider claude --samples logs.txt
```

## API overview

- `infer_schema(provider, &[samples], max_retries) -> Result<String, InferenceError>`
  — validate-and-retry suggestion loop.
- `InferenceProvider` trait, `AttemptContext` — provider contract.
- `provider_by_name(name) -> Result<Box<dyn InferenceProvider>, InferenceError>`.
- `clients::{AnthropicProvider, OpenAiCompatProvider, OllamaProvider,
  MockProvider, FlakyMockProvider}` — bundled providers.
- `prompt::{SYSTEM_PROMPT, build_user_prompt, strip_code_fences}` — prompt building.
- `error::{InferenceError, AttemptContext}` — error types (incl. `NoApiKey`,
  `ValidationFailed`).

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
