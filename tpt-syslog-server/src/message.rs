//! Parsed syslog message envelope produced by the receivers.

use std::net::SocketAddr;
use std::time::SystemTime;

/// Transport the message arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Udp,
    Tcp,
}

/// Framing used to delimit the message on its transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// One syslog message per UDP datagram (RFC3164 / RFC5424-over-UDP).
    Datagram,
    /// RFC3164-style non-transparent framing: a message terminated by `LF`.
    Rfc3164Lf,
    /// RFC5424 octet-counting: `<len> SP <len octets>`.
    Rfc5424OctetCounting,
}

/// A single received syslog message.
#[derive(Debug, Clone)]
pub struct Message {
    pub transport: Transport,
    pub remote: SocketAddr,
    pub framing: Framing,
    /// Raw message bytes (no framing delimiters).
    pub payload: Vec<u8>,
    pub received_at: SystemTime,
}
