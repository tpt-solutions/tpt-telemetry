//! `tpt-telemetry-compiler` — `.tpt-log` schema → zero-copy runtime parser.
//!
//! The compiler lowers a parsed [`Schema`] into a
//! [`CompiledSchema`], a flat, allocation-free matcher that borrows field values
//! directly from the input line. It also emits Rust source via [`codegen`] for
//! golden-file testing and `build.rs` integration.
//!
//! # `build.rs` integration
//!
//! ```no_run
//! // build.rs
//! use std::env;
//! use std::fs;
//! use std::path::PathBuf;
//!
//! fn main() {
//!     let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("tpt_schema.rs");
//!     let src = tpt_telemetry_compiler::build::codegen_schema_file("schemas/asa.tpt-log")
//!         .expect("codegen failed");
//!     fs::write(&out, src).unwrap();
//!     println!("cargo:rerun-if-changed=schemas/asa.tpt-log");
//! }
//!
//! // lib.rs
//! // include!(concat!(env!("OUT_DIR"), "/tpt_schema.rs"));
//! ```

pub mod codegen;
pub mod compile;
pub mod error;
pub mod security;

pub use compile::{
    CompiledFormat, CompiledSchema, Field, MatchCtx, RawMatch, Record, Seg, TypedField, Value,
};
pub use error::{CompileError, Result};

// Re-export the schema AST types the generated code references directly.
pub use tpt_telemetry_schema::ast::{CoercionTarget, RedactMode, TypeName};
pub use tpt_telemetry_schema::{parse, Schema};

/// `build.rs` integration helpers.
pub mod build {
    use crate::codegen;
    use crate::error::{CompileError, Result};
    use std::path::Path;

    /// Parse a `.tpt-log` file and return generated Rust source for it.
    pub fn codegen_schema_file(path: impl AsRef<Path>) -> Result<String> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| CompileError::Codegen(e.to_string()))?;
        let schema = crate::parse(&text)?;
        codegen::generate_rust(&schema)
    }

    /// Parse a `.tpt-log` source string and return generated Rust source.
    pub fn codegen_schema_str(src: &str) -> Result<String> {
        let schema = crate::parse(src)?;
        codegen::generate_rust(&schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_compile_and_parse() {
        let src = r#"
            format CiscoASA {
              pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
              coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
            }
        "#;
        let schema = parse(src).unwrap();
        let cs = CompiledSchema::compile(&schema).unwrap();
        let rec = cs
            .parse_line("%ASA-6-302013: Built inbound TCP connection")
            .unwrap();
        assert_eq!(rec.format, "CiscoASA");
        let sev = rec.fields.iter().find(|f| f.name == "severity").unwrap();
        assert_eq!(sev.value, Value::Enum(6)); // INFO
        let msg = rec.fields.iter().find(|f| f.name == "message").unwrap();
        assert_eq!(msg.value, Value::Str("Built inbound TCP connection"));
    }
}
