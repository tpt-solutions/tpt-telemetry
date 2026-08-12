//! Generates the golden Rust source for `tests/golden/cisco_asa.rs`.
//!
//! Run with: `cargo run -p tpt-telemetry-compiler --example gen_golden`

fn main() {
    let src = include_str!("../../examples/schemas/cisco_asa.tpt-log");
    let schema = tpt_telemetry_schema::parse(src).expect("parse schema");
    let generated = tpt_telemetry_compiler::codegen::generate_rust(&schema).expect("codegen");
    print!("{generated}");
}
