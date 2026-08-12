#![no_main]
//! Fuzz target: compiling arbitrary Grok pattern strings and scanning arbitrary
//! sample lines must never panic (only ever return `Err`/`None`).

use libfuzzer_sys::fuzz_target;
use tpt_grok_engine::Grok;

fuzz_target!(|data: &[u8]| {
    let Ok(pattern) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(g) = Grok::new(pattern) else {
        return;
    };
    // A fixed-but-representative sample exercises the regex hot path.
    let sample = "192.168.1.1 accepted 4096 GET /index.html 200";
    let _ = g.scan(sample);
    let _ = g.find(sample);
});
