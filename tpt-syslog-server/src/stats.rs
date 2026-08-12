//! Shared, atomically-updated server statistics.

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters surfaced by [`crate::SyslogServer::stats`].
#[derive(Default)]
pub struct Stats {
    /// Messages delivered into the ring buffer (not dropped).
    pub delivered: AtomicU64,
    /// Messages dropped because the ring buffer was full (backpressure).
    pub dropped: AtomicU64,
    /// Kernel-reported socket receive-queue overflow count (Linux `SO_RXQ_OVFL`).
    pub kernel_overflow: AtomicU64,
}

impl Stats {
    pub(crate) fn new() -> Self {
        Stats::default()
    }

    /// Snapshot the counters into a plain struct for inspection.
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            delivered: self.delivered.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            kernel_overflow: self.kernel_overflow.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time copy of [`Stats`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub delivered: u64,
    pub dropped: u64,
    pub kernel_overflow: u64,
}
