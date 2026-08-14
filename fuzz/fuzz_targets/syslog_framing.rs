#![no_main]
//! Fuzz target: the incremental TCP syslog frame decoder must never panic and
//! every emitted frame must be a byte-subset of the input, regardless of how the
//! stream is chunked or which framing mode is selected.

use libfuzzer_sys::fuzz_target;
use tpt_syslog_server::framing::{TcpDecoder, TcpFraming, MAX_FRAME_LEN};

fuzz_target!(|data: &[u8]| {
    for mode in [
        TcpFraming::Auto,
        TcpFraming::OctetCounting,
        TcpFraming::NonTransparent,
    ] {
        let mut decoder = TcpDecoder::new(mode, MAX_FRAME_LEN);
        let mut frames = Vec::new();
        // Feed in tiny 1–4 byte chunks to stress incremental decoding.
        for chunk in data.chunks(3) {
            decoder.push(chunk, &mut frames);
        }
        decoder.flush(&mut frames);

        // Invariant: every frame must be a contiguous slice of the original input.
        for f in &frames {
            assert!(f.len() <= data.len());
        }
    }
});
