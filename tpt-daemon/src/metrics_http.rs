//! Hand-rolled, dependency-free HTTP/1.1 responder for daemon metrics and
//! health checks. Exposes `GET /metrics` (Prometheus text format, sourced from
//! the syslog server stats plus daemon-level counters) and `GET /healthz`
//! (heartbeat readiness).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tpt_syslog_server::SyslogServer;

/// Run the metrics HTTP server until `stop` is set. Binds the already-bound
/// `listener` and answers `GET /metrics` and `GET /healthz`.
pub fn run_metrics(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    server: Arc<Mutex<SyslogServer>>,
    received: Arc<AtomicU64>,
    exported: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
) {
    let _ = listener.set_nonblocking(true);
    let mut buf = [0u8; 4096];
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut req = Vec::new();
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            req.extend_from_slice(&buf[..n]);
                            if req.windows(4).any(|w| w == b"\r\n\r\n") || req.len() > 8192 {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let path = extract_path(&req);
                let (status, body) = match path.as_deref() {
                    Some("/healthz") => ("200 OK", "ok\n".to_string()),
                    Some("/metrics") => (
                        "200 OK",
                        render_metrics(&server.lock().unwrap(), &received, &exported, &errors),
                    ),
                    _ => ("404 Not Found", "not found\n".to_string()),
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn extract_path(req: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(req).ok()?;
    let line = s.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    if method != "GET" {
        return None;
    }
    let p = parts.next()?;
    Some(p.to_string())
}

fn metric(m: &mut String, name: &str, help: &str, value: u64) {
    m.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {value}\n"
    ));
}

fn render_metrics(
    server: &SyslogServer,
    received: &AtomicU64,
    exported: &AtomicU64,
    errors: &AtomicU64,
) -> String {
    let s = server.stats();
    let mut m = String::new();
    metric(
        &mut m,
        "tpt_daemon_received_total",
        "Syslog messages received by the daemon",
        received.load(Ordering::Relaxed),
    );
    metric(
        &mut m,
        "tpt_daemon_exported_total",
        "Records exported to OTLP",
        exported.load(Ordering::Relaxed),
    );
    metric(
        &mut m,
        "tpt_daemon_export_errors_total",
        "OTLP export errors",
        errors.load(Ordering::Relaxed),
    );
    metric(
        &mut m,
        "tpt_daemon_delivered_total",
        "Messages delivered into the syslog ring buffer",
        s.delivered,
    );
    metric(
        &mut m,
        "tpt_daemon_dropped_full_total",
        "Ring-buffer-full drops (backpressure)",
        s.dropped_full,
    );
    metric(
        &mut m,
        "tpt_daemon_dropped_disconnected_total",
        "Consumer-disconnected drops",
        s.dropped_disconnected,
    );
    metric(
        &mut m,
        "tpt_daemon_kernel_overflow_total",
        "Kernel RXQ overflows (Linux SO_RXQ_OVFL)",
        s.kernel_overflow,
    );
    metric(
        &mut m,
        "tpt_daemon_rejected_connections_total",
        "TCP connections rejected at the max_connections cap",
        s.rejected_connections,
    );
    m
}
