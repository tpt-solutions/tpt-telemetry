//! `tpt-daemon` — unified syslog ingest → parse → OTLP export daemon.
//!
//! Wires the syslog receiver ([`tpt_syslog_server`]), the schema-driven parser
//! ([`tpt_telemetry_core`]) and the OTLP exporter ([`tpt_otlp`]) into a single
//! long-running process with a Prometheus metrics endpoint and health check.

pub mod config;
pub mod metrics_http;

use anyhow::{Context, Result};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tpt_otlp::Exporter;
use tpt_syslog_server::SyslogServer;
use tpt_telemetry_core::Parser;
use tpt_telemetry_schema::load_file;

pub use config::DaemonConfig;

/// A configured, running daemon. Construct with [`Daemon::new`], then [`Daemon::start`]
/// to bind listeners and spawn the worker + metrics threads.
pub struct Daemon {
    server: Arc<Mutex<SyslogServer>>,
    parser: Parser,
    exporter: Exporter,
    stop: Arc<AtomicBool>,
    metrics_listener: TcpListener,
    metrics_addr: SocketAddr,
    received: Arc<AtomicU64>,
    exported: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
}

impl Daemon {
    /// Build a daemon from configuration: bind the syslog + metrics listeners,
    /// compile the schema, and construct the OTLP exporter.
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let server = Arc::new(Mutex::new(
            SyslogServer::new(config.server_config()).context("failed to bind syslog listeners")?,
        ));
        let schema = load_file(&config.schema.path).context("failed to load schema file")?;
        let parser = Parser::new(schema).context("failed to compile schema")?;
        let exporter = Exporter::new(config.exporter_config());

        let metrics_listener =
            TcpListener::bind(&config.metrics.bind).context("failed to bind metrics listener")?;
        let metrics_addr = metrics_listener
            .local_addr()
            .context("metrics local addr")?;

        Ok(Daemon {
            server,
            parser,
            exporter,
            stop: Arc::new(AtomicBool::new(false)),
            metrics_listener,
            metrics_addr,
            received: Arc::new(AtomicU64::new(0)),
            exported: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Local UDP bind address (useful when bound to port 0).
    pub fn local_udp_addr(&self) -> SocketAddr {
        self.server.lock().unwrap().local_udp_addr()
    }

    /// Local TCP bind address (useful when bound to port 0).
    pub fn local_tcp_addr(&self) -> SocketAddr {
        self.server.lock().unwrap().local_tcp_addr()
    }

    /// Local metrics bind address (useful when bound to port 0).
    pub fn local_metrics_addr(&self) -> SocketAddr {
        self.metrics_addr
    }

    /// Shared shutdown flag; set to request graceful shutdown.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Start the worker and metrics threads. Consumes `self` and returns a
    /// [`RunningDaemon`] handle exposing the bound addresses and a `stop()` method.
    pub fn start(self) -> RunningDaemon {
        let udp = self.server.lock().unwrap().local_udp_addr();
        let tcp = self.server.lock().unwrap().local_tcp_addr();
        let metrics = self.metrics_addr;
        let stop = self.stop.clone();
        let received = self.received.clone();
        let exported = self.exported.clone();
        let errors = self.errors.clone();

        let metrics_thread = {
            let stop = stop.clone();
            let server = self.server.clone();
            let received = received.clone();
            let exported = exported.clone();
            let errors = errors.clone();
            thread::spawn(move || {
                metrics_http::run_metrics(
                    self.metrics_listener,
                    stop,
                    server,
                    received,
                    exported,
                    errors,
                );
            })
        };

        let worker = thread::spawn(move || {
            run_worker(
                self.server,
                self.parser,
                self.exporter,
                stop,
                received,
                exported,
                errors,
            );
        });

        RunningDaemon {
            udp,
            tcp,
            metrics,
            stop: self.stop.clone(),
            worker: Some(worker),
            metrics_thread: Some(metrics_thread),
        }
    }
}

/// Handle to a running daemon.
pub struct RunningDaemon {
    /// Local UDP bind address.
    pub udp: SocketAddr,
    /// Local TCP bind address.
    pub tcp: SocketAddr,
    /// Local metrics bind address.
    pub metrics: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    metrics_thread: Option<JoinHandle<()>>,
}

impl RunningDaemon {
    /// Request graceful shutdown (also happens automatically on `Drop`).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for RunningDaemon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
        if let Some(h) = self.metrics_thread.take() {
            let _ = h.join();
        }
    }
}

/// Main processing loop: drain the syslog ring buffer, UTF-8 decode, parse, and
/// export each record to OTLP. The exporter performs its own internal batching,
/// retry, and backoff.
fn run_worker(
    server: Arc<Mutex<SyslogServer>>,
    parser: Parser,
    exporter: Exporter,
    stop: Arc<AtomicBool>,
    received: Arc<AtomicU64>,
    exported: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
) {
    while !stop.load(Ordering::SeqCst) {
        let msg = {
            let s = server.lock().unwrap();
            match s.recv_timeout(Duration::from_millis(100)) {
                Ok(m) => Some(m),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };
        if let Some(msg) = msg {
            received.fetch_add(1, Ordering::Relaxed);
            let text = String::from_utf8_lossy(&msg.payload);
            if let Some(rec) = parser.parse_line(&text) {
                match exporter.export(std::slice::from_ref(&rec)) {
                    Ok(()) => {
                        exported.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(error = %e, "otlp export failed");
                    }
                }
            }
        }
    }
    tracing::info!("daemon worker stopped");
}
