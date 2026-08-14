# tpt-syslog-server

High-throughput UDP/TCP syslog receiver (RFC3164 / RFC5424) for
[`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Implements RFC3164 (UDP datagrams / TCP non-transparent LF framing) and RFC5424
(TCP octet-counting framing) reception. Incoming messages are pushed into a bounded
ring buffer (a `sync_channel`) so memory is bounded under log floods; when the
buffer is full, messages are dropped and counted — the application-level backpressure
signal. On Linux the kernel-level `SO_RXQ_OVFL` drop counter is enabled and surfaced
via the stats API.

## Features

- **Dual transport** — UDP (datagrams) and TCP (octet-counting or LF framing, or
  auto-detect) receivers bound independently.
- **Bounded memory** — a ring buffer of configurable capacity provides backpressure;
  overflow drops are counted, never unbounded.
- **Connection caps** — maximum concurrent TCP connections; excess connections are
  accepted then closed and counted as rejected.
- **Frame limits** — per-frame byte ceiling enforced by the TCP framing decoder.
- **Kernel overflow accounting** — Linux `SO_RXQ_OVFL` drop counter surfaced via
  `Stats`.
- **Observability** — live `Stats` snapshot (received, dropped, rejected, errors).

## Installation

```toml
[dependencies]
tpt-syslog-server = "0.1.0"
```

## Usage

### Starting a server

```rust
use tpt_syslog_server::{ServerConfig, SyslogServer, TcpFraming};

let config = ServerConfig::localhost(514, 601); // udp 514, tcp 601
let server = SyslogServer::new(config).expect("bind listeners");
println!("udp: {}, tcp: {}", server.local_udp_addr(), server.local_tcp_addr());
```

### Consuming messages

```rust
use std::time::Duration;
use tpt_syslog_server::SyslogServer;

let server = SyslogServer::new(ServerConfig::localhost(0, 0)).unwrap();
// In a worker thread:
loop {
    match server.recv_timeout(Duration::from_millis(100)) {
        Ok(msg) => println!("[{:?}] {}", msg.transport, String::from_utf8_lossy(&msg.payload)),
        Err(_) => break, // timeout or shutdown
    }
}
```

### Configuration knobs

| Field | Default | Meaning |
|-------|---------|---------|
| `udp_bind` / `tcp_bind` | `127.0.0.1:0` | bind addresses |
| `ring_capacity` | `1 << 16` | backpressure ring size (messages) |
| `read_timeout_ms` | `250` | socket read timeout (shutdown polling) |
| `tcp_framing` | `TcpFraming::Auto` | `Auto` / `OctetCounting` / `NonTransparent` |
| `max_connections` | `1024` | concurrent TCP connection ceiling |
| `max_frame_len` | `1_048_576` | per-frame byte ceiling |

Constants `DEFAULT_RING_CAPACITY`, `DEFAULT_MAX_CONNECTIONS`, `DEFAULT_MAX_FRAME_LEN`,
and `DEFAULT_READ_TIMEOUT_MS` are re-exported from the crate root.

### Reading stats

```rust
use tpt_syslog_server::SyslogServer;

let server = SyslogServer::new(ServerConfig::localhost(0, 0)).unwrap();
let snap = server.stats().snapshot();
println!("received={} dropped={}", snap.received, snap.dropped);
```

## API overview

- `SyslogServer::new(ServerConfig) -> io::Result<Self>` — bind and start workers.
- `SyslogServer::local_udp_addr()`, `local_tcp_addr()` — bound addresses.
- `SyslogServer::recv_timeout(Duration) -> Result<Message, RecvTimeoutError>` — pull a message.
- `SyslogServer::stats() -> Arc<Stats>` — live statistics.
- `ServerConfig`, `ServerConfig::localhost(udp, tcp)`, `TcpFraming` — configuration.
- `Message`, `Transport`, `Framing` — received message + metadata types.
- `framing::TcpFraming`, `framing::FrameError` — framing decoder.
- `Stats`, `StatsSnapshot` — counters.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
