#![no_main]
//! Fuzz target: the highest-risk unsafe path in the syslog server is the
//! UDP `recvmsg` ancillary-data / `sockaddr` parsing on Linux. The actual
//! `recvmsg` call cannot be driven from fuzz bytes, but the *parsing* it
//! performs — decoding a raw `sockaddr_*` body and reading the `SCM_RXQ_OVFL`
//! overflow counter out of the control buffer — is exercised here with
//! arbitrary bytes. Both helpers are bounds-checked and must never panic (only
//! ever return `None`).

use libfuzzer_sys::fuzz_target;
use tpt_syslog_server::{decode_sockaddr, parse_rxq_ovfl};

fuzz_target!(|data: &[u8]| {
    // Split into a (family, sockaddr body) pair: the family byte is taken from
    // the first byte (0 if empty) and the rest is the raw address buffer,
    // mirroring how `sockaddr_to_addr` feeds `decode_sockaddr`.
    let family = data.first().copied().unwrap_or(0);
    let body = if data.is_empty() { &[][..] } else { &data[1..] };
    let _ = decode_sockaddr(family, body);

    // The whole buffer stands in for a cmsg data payload; the helper must reject
    // short inputs instead of reading out of bounds (it replaces the unsafe
    // `*(CMSG_DATA(cm) as *const u32)` read in the Linux receive loop).
    let _ = parse_rxq_ovfl(data);
});
