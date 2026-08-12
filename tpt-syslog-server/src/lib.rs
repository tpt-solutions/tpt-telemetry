//! `tpt-syslog-server` — high-throughput UDP/TCP syslog receiver.
//!
//! Implements RFC3164 (UDP datagrams / TCP non-transparent LF framing) and
//! RFC5424 (TCP octet-counting framing) reception. Incoming messages are pushed
//! into a bounded ring buffer (a `sync_channel`) so memory is bounded under log
//! floods; when the buffer is full, messages are dropped and counted (the
//! application-level backpressure signal). On Linux the kernel-level
//! `SO_RXQ_OVFL` drop counter is enabled and surfaced via the stats API.

pub mod framing;
pub mod message;
mod server;
mod stats;

pub use framing::TcpFraming;
pub use message::{Framing, Message, Transport};
pub use server::{ServerConfig, SyslogServer};
pub use stats::Stats;

/// Default ring-buffer capacity (in messages) used for backpressure.
pub const DEFAULT_RING_CAPACITY: usize = 1 << 16;

/// Default socket read timeout used to periodically check the shutdown flag.
pub const DEFAULT_READ_TIMEOUT_MS: u64 = 250;
