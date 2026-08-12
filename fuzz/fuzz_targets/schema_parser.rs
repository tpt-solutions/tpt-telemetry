#![no_main]
//! Fuzz target: the `.tpt-log` schema parser must never panic on arbitrary
//! input. Valid schemas are additionally compiled to exercise the AST →
//! `CompiledSchema` lowering path.

use libfuzzer_sys::fuzz_target;
use tpt_telemetry_compiler::CompiledSchema;
use tpt_telemetry_schema::parse;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(schema) = parse(s) {
        // Compiling a valid schema must also be panic-free.
        let _ = CompiledSchema::compile(&schema);
    }
});
