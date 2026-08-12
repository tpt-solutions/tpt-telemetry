//! The inference-provider trait and the validate-and-retry inference loop.

use crate::error::{AttemptContext, InferenceError};
use tpt_telemetry_compiler::CompiledSchema;
use tpt_telemetry_schema::Schema;

/// A provider turns raw log samples into a candidate `.tpt-log` schema.
pub trait InferenceProvider {
    /// Provider identifier (e.g. `"claude"`, `"openai"`).
    fn name(&self) -> &str;

    /// Suggest a schema for the given samples. `context` carries prior-attempt
    /// feedback during the validation loop (may be empty).
    fn suggest(&self, samples: &[&str], context: &str) -> Result<String, InferenceError>;

    /// Validate that the suggested schema text parses **and** compiles via the
    /// Phase 3 compiler.
    fn validate(&self, schema_text: &str) -> Result<Schema, InferenceError> {
        let schema = tpt_telemetry_schema::parse(schema_text)
            .map_err(|e| InferenceError::InvalidSchema(e.to_string()))?;
        CompiledSchema::compile(&schema).map_err(|e| InferenceError::Compile(e.to_string()))?;
        Ok(schema)
    }
}

/// Run the provider in a validate-and-retry loop: suggest, validate (parse +
/// compile), and feed compilation errors back to the model up to `max_retries`
/// times. Returns the first schema that parses and compiles.
pub fn infer_schema(
    provider: &dyn InferenceProvider,
    samples: &[&str],
    max_retries: usize,
) -> Result<String, InferenceError> {
    use crate::prompt::{build_user_prompt, with_feedback};
    let base_prompt = build_user_prompt(samples);

    let mut attempts: Vec<AttemptContext> = Vec::new();
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        let ctx = with_feedback(&base_prompt, &attempts);
        let text = provider.suggest(samples, &ctx)?;
        match provider.validate(&text) {
            Ok(_) => return Ok(text),
            Err(e) => {
                last_error = e.to_string();
                attempts.push(AttemptContext {
                    attempt,
                    error: last_error.clone(),
                    schema_text: text,
                });
            }
        }
    }
    Err(InferenceError::ValidationFailed(last_error))
}
