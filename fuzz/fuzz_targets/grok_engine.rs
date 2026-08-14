#![no_main]
//! Fuzz target: compiling arbitrary Grok pattern strings and scanning arbitrary
//! sample lines must never panic (only ever return `Err`/`None`).

use libfuzzer_sys::fuzz_target;
use tpt_grok_engine::Grok;

fuzz_target!(|data: &[u8]| {
    // Split the fuzz bytes into a candidate pattern and candidate input text so
    // both the *pattern* and the *scanned input* are adversarial (the latter
    // gives ReDoS-style coverage of the regex hot path).
    let split = data.len() / 2;
    let (pat, input) = data.split_at(split);

    let Ok(pattern) = std::str::from_utf8(pat) else {
        return;
    };
    // A fixed-but-representative sample still exercises the regex hot path.
    let sample = "192.168.1.1 accepted 4096 GET /index.html 200";
    let Ok(g) = Grok::new(pattern) else {
        return;
    };
    let _ = g.scan(sample);
    let _ = g.find(sample);

    // Now scan the adversarial input text itself; the engine must return
    // (Err/None) and never panic on adversarial input.
    let Ok(text) = std::str::from_utf8(input) else {
        return;
    };
    let _ = g.scan(text);
    let _ = g.find(text);
});
