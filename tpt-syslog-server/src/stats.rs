//! Shared, atomically-updated server statistics.

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters surfaced by [`crate::SyslogServer::stats`].
#[derive(Default)]
pub struct Stats {
    /// Messages delivered into the ring buffer (not dropped).
    pub delivered: AtomicU64,
    /// Messages dropped because the ring buffer was full (backpressure).
    pub dropped_full: AtomicU64,
    /// Messages dropped because the downstream consumer disconnected.
    pub dropped_disconnected: AtomicU64,
    /// Kernel-reported socket receive-queue overflow count (Linux `SO_RXQ_OVFL`).
    pub kernel_overflow: AtomicU64,
    /// TCP connections rejected because the server was at its `max_connections` cap.
    pub rejected_connections: AtomicU64,
}

impl Stats {
    pub(crate) fn new() -> Self {
        Stats::default()
    }

    /// Snapshot the counters into a plain struct for inspection.
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            delivered: self.delivered.load(Ordering::Relaxed),
            dropped_full: self.dropped_full.load(Ordering::Relaxed),
            dropped_disconnected: self.dropped_disconnected.load(Ordering::Relaxed),
            kernel_overflow: self.kernel_overflow.load(Ordering::Relaxed),
            rejected_connections: self.rejected_connections.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time copy of [`Stats`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub delivered: u64,
    pub dropped_full: u64,
    pub dropped_disconnected: u64,
    pub kernel_overflow: u64,
    pub rejected_connections: u64,
}
