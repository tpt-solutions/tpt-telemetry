//! Integration tests for the syslog server: UDP datagrams, TCP LF framing, and
//! TCP octet-counting framing, plus the ring-buffer backpressure/drop behavior.

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
    assert!(msgs
        .iter()
        .any(|m| m.payload.starts_with(b"udp message")));
    server.stop();
}

#[test]
fn tcp_non_transparent_lf() {
    let mut cfg = ServerConfig::default();
    cfg.tcp_framing = tpt_syslog_server::TcpFraming::NonTransparent;
    let server = SyslogServer::new(cfg).unwrap();
    let target = server.local_tcp_addr();

    let mut stream = TcpStream::connect(target).unwrap();
    stream
        .write_all(b"first line\nsecond line\n")
        .unwrap();
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
    let mut cfg = ServerConfig::default();
    cfg.tcp_framing = tpt_syslog_server::TcpFraming::OctetCounting;
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
    let mut cfg = ServerConfig::default();
    cfg.ring_capacity = 2;
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
    assert!(stats.dropped > 0, "expected backpressure drops");
    server.stop();
}
