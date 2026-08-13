//! Integration tests for the syslog server: UDP datagrams, TCP LF framing, and
//! TCP octet-counting framing, plus the ring-buffer backpressure/drop behavior
//! and the security-hardening caps (max connections, max frame length).

use std::io::Write;
use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

use tpt_syslog_server::{Framing, Message, ServerConfig, SyslogServer, Transport};

fn recv_within(server: &SyslogServer, n: usize, timeout: Duration) -> Vec<Message> {
    let mut out = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    while out.len() < n && std::time::Instant::now() < deadline {
        match server.recv_timeout(Duration::from_millis(100)) {
            Ok(m) => out.push(m),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}

#[test]
fn udp_datagrams() {
    let server = SyslogServer::new(ServerConfig::default()).unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let target = server.local_udp_addr();

    for i in 0..5 {
        let line = format!("udp message {i}");
        udp.send_to(line.as_bytes(), target).unwrap();
    }

    let msgs = recv_within(&server, 5, Duration::from_secs(3));
    assert_eq!(msgs.len(), 5);
    assert!(msgs.iter().all(|m| m.transport == Transport::Udp));
    assert!(msgs.iter().all(|m| m.framing == Framing::Datagram));
    assert!(msgs.iter().any(|m| m.payload.starts_with(b"udp message")));
    server.stop();
}

#[test]
fn tcp_non_transparent_lf() {
    let cfg = ServerConfig {
        tcp_framing: tpt_syslog_server::TcpFraming::NonTransparent,
        ..Default::default()
    };
    let server = SyslogServer::new(cfg).unwrap();
    let target = server.local_tcp_addr();

    let mut stream = TcpStream::connect(target).unwrap();
    stream.write_all(b"first line\nsecond line\n").unwrap();
    // Let the reader process.
    std::thread::sleep(Duration::from_millis(100));

    let msgs = recv_within(&server, 2, Duration::from_secs(3));
    let bodies: Vec<&[u8]> = msgs.iter().map(|m| m.payload.as_slice()).collect();
    assert!(bodies.iter().any(|b| *b == b"first line"));
    assert!(bodies.iter().any(|b| *b == b"second line"));
    assert!(msgs.iter().all(|m| m.framing == Framing::Rfc3164Lf));
    server.stop();
}

#[test]
fn tcp_octet_counting() {
    let cfg = ServerConfig {
        tcp_framing: tpt_syslog_server::TcpFraming::OctetCounting,
        ..Default::default()
    };
    let server = SyslogServer::new(cfg).unwrap();
    let target = server.local_tcp_addr();

    let mut stream = TcpStream::connect(target).unwrap();
    // Each payload "octet counted frame one" / "... two" is 23 bytes.
    let frame = b"23 octet counted frame one23 octet counted frame two";
    stream.write_all(frame).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let msgs = recv_within(&server, 2, Duration::from_secs(3));
    let bodies: Vec<&[u8]> = msgs.iter().map(|m| m.payload.as_slice()).collect();
    assert!(bodies.iter().any(|b| *b == b"octet counted frame one"));
    assert!(bodies.iter().any(|b| *b == b"octet counted frame two"));
    assert!(msgs
        .iter()
        .all(|m| m.framing == Framing::Rfc5424OctetCounting));
    server.stop();
}

#[test]
fn backpressure_drops_over_full_ring() {
    // Tiny ring (2 messages) so the flood is guaranteed to overflow.
    let cfg = ServerConfig {
        ring_capacity: 2,
        ..Default::default()
    };
    let server = SyslogServer::new(cfg).unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let target = server.local_udp_addr();

    // Stop consuming; flood the socket.
    let burst = 200u32;
    for i in 0..burst {
        let line = format!("flood {i}");
        let _ = udp.send_to(line.as_bytes(), target);
    }
    // Give the receiver time to process + drop.
    std::thread::sleep(Duration::from_millis(200));
    let stats = server.stats();
    assert!(stats.dropped_full > 0, "expected backpressure drops");
    server.stop();
}

#[test]
fn connection_flood_rejected_at_cap() {
    // Cap concurrent connections at 2; opening more must be rejected.
    let cfg = ServerConfig {
        max_connections: 2,
        ..Default::default()
    };
    let server = SyslogServer::new(cfg).unwrap();
    let target = server.local_tcp_addr();

    // Open more connections than the cap allows. The surplus are accepted then
    // immediately closed and counted as rejected (the TCP connect itself still
    // succeeds because accept happens before the cap check).
    let _streams: Vec<TcpStream> = (0..5)
        .map(|_| TcpStream::connect(target).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(300));

    let stats = server.stats();
    assert!(
        stats.rejected_connections >= 3,
        "expected >=3 rejected connections, got {}",
        stats.rejected_connections
    );
    server.stop();
}

#[test]
fn oversized_octet_count_frame_dropped() {
    let cfg = ServerConfig {
        tcp_framing: tpt_syslog_server::TcpFraming::OctetCounting,
        max_frame_len: 64,
        ..Default::default()
    };
    let server = SyslogServer::new(cfg).unwrap();
    let target = server.local_tcp_addr();

    // Claim a 1 MiB frame far above the 64-byte ceiling, with no payload.
    let mut bad = TcpStream::connect(target).unwrap();
    let _ = bad.write_all(b"1048576 ");
    std::thread::sleep(Duration::from_millis(200));

    // The framing decoder must reject it without delivering a huge frame.
    let rejected = recv_within(&server, 1, Duration::from_millis(200));
    assert!(rejected.is_empty(), "oversized frame must not be delivered");

    // A fresh connection with a valid small frame still works (the bad one was
    // closed, not used to grow our buffer without bound).
    let mut good = TcpStream::connect(target).unwrap();
    good.write_all(b"5 hello").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let good = recv_within(&server, 1, Duration::from_secs(2));
    assert!(
        good.iter().any(|m| m.payload.as_slice() == b"hello"),
        "valid frame after rejection must still be delivered"
    );
    server.stop();
}
