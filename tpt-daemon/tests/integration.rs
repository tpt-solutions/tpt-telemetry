//! End-to-end integration test: a UDP syslog datagram is received, parsed via the
//! compiled schema, and exported to a stub local OTLP/HTTP collector.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tpt_daemon::{Daemon, DaemonConfig};

/// Start a minimal TCP server that mimics an OTLP/HTTP collector: it accepts one
/// connection, reads the POST, and records the number of bytes received.
fn stub_collector() -> (std::net::SocketAddr, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let got = Arc::new(AtomicU64::new(0));
    let got2 = got.clone();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let mut total = 0usize;
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        if total > 64 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            got2.fetch_add(total as u64, Ordering::SeqCst);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        }
    });
    (addr, got)
}

fn schema_path() -> PathBuf {
    // Tests run with CWD == crate dir; examples live at the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("schemas")
        .join("cisco_asa.tpt-log")
}

#[test]
fn udp_datagram_parsed_and_exported_to_collector() {
    let (stub_addr, got) = stub_collector();

    // Use forward slashes so the path is a valid TOML string on Windows.
    let schema = schema_path().to_string_lossy().replace('\\', "/");

    let cfg_toml = format!(
        r#"
        [schema]
        path = "{path}"

        [syslog.udp]
        bind = "127.0.0.1:0"

        [syslog.tcp]
        bind = "127.0.0.1:0"

        [otlp]
        endpoint = "http://{stub_addr}"
        transport = "http"

        [metrics]
        bind = "127.0.0.1:0"

        [logging]
        level = "info"
        "#,
        path = schema,
        stub_addr = stub_addr,
    );

    let mut cfg: DaemonConfig = toml::from_str(&cfg_toml).unwrap();
    cfg.interpolate_env();

    let daemon = Daemon::new(cfg).unwrap();
    let udp = daemon.local_udp_addr();
    let running = daemon.start();

    // Emit a Cisco ASA line that matches the example schema.
    let line = "%ASA-6-302013: Built inbound TCP connection";
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.send_to(line.as_bytes(), udp).unwrap();

    // Wait for the stub collector to receive the export.
    let deadline = Instant::now() + Duration::from_secs(10);
    while got.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        got.load(Ordering::SeqCst) > 0,
        "stub OTLP collector did not receive an export"
    );

    running.stop();
}

#[test]
fn example_config_parses() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("packaging")
        .join("tpt-daemon.example.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut cfg: DaemonConfig = toml::from_str(&text).unwrap();
    cfg.interpolate_env();
    assert_eq!(cfg.otlp.transport, tpt_daemon::config::OtlpTransport::Http);
    assert_eq!(cfg.otlp.endpoint, "http://localhost:4318");
    assert_eq!(cfg.syslog.tcp.max_frame_len, 1048576);
    assert_eq!(cfg.metrics.bind, "0.0.0.0:9464");
}

/// End-to-end integration test against a **real** OpenTelemetry Collector
/// (closes the deferred Phase 7 live-collector item).
///
/// This is gated behind `TPT_OTEL_COLLECTOR_TEST=1` because it requires a
/// running collector reachable at `http://127.0.0.1:4318` (OTLP/HTTP) with its
/// self-metrics exposed on `http://127.0.0.1:8889/metrics`. The CI `otel-
/// collector` job provisions this via a containerized collector and sets the
/// flag, so the test runs there but is skipped in ordinary `cargo test`.
#[test]
fn exported_to_real_otel_collector() {
    if std::env::var("TPT_OTEL_COLLECTOR_TEST").is_err() {
        eprintln!(
            "skipping exported_to_real_otel_collector: \
             set TPT_OTEL_COLLECTOR_TEST=1 with an otel-collector on :4318 and :8889"
        );
        return;
    }

    let schema = schema_path().to_string_lossy().replace('\\', "/");
    let cfg_toml = format!(
        r#"
        [schema]
        path = "{path}"

        [syslog.udp]
        bind = "127.0.0.1:0"

        [syslog.tcp]
        bind = "127.0.0.1:0"

        [otlp]
        endpoint = "http://127.0.0.1:4318"
        transport = "http"

        [metrics]
        bind = "127.0.0.1:0"

        [logging]
        level = "info"
        "#,
        path = schema,
    );

    let mut cfg: DaemonConfig = toml::from_str(&cfg_toml).unwrap();
    cfg.interpolate_env();

    let daemon = Daemon::new(cfg).unwrap();
    let udp = daemon.local_udp_addr();
    let running = daemon.start();

    let line = "%ASA-6-302013: Built inbound TCP connection";
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.send_to(line.as_bytes(), udp).unwrap();

    // Poll the collector's Prometheus self-metrics for accepted log records.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut accepted = 0u64;
    while Instant::now() < deadline {
        accepted = collector_accepted_log_records("127.0.0.1:8889");
        if accepted > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(
        accepted > 0,
        "otel-collector did not report any accepted log records"
    );

    running.stop();
}

/// Fetch the collector's `/metrics` endpoint and sum the
/// `otelcol_receiver_accepted_log_records` gauge (across all label sets).
fn collector_accepted_log_records(addr: &str) -> u64 {
    let Ok(mut stream) = TcpStream::connect(addr) else {
        return 0;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!("GET /metrics HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return 0;
    }
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    let body = String::from_utf8_lossy(&buf);

    let mut total = 0u64;
    for line in body.lines() {
        if line.starts_with("otelcol_receiver_accepted_log_records") && !line.starts_with('#') {
            if let Some(value) = line.split_whitespace().last() {
                if let Ok(v) = value.parse::<f64>() {
                    total += v as u64;
                }
            }
        }
    }
    total
}
