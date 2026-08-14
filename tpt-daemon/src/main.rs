//! `tpt-daemon` binary entry point.
//!
//! Supports `--config <path>` (required for normal operation), plus the
//! operational subcommands used by container/CI tooling:
//!
//! - `--help` / `-h`         — print usage and exit 0.
//! - `--version` / `-V`      — print the version and exit 0.
//! - `--check` / `--validate-config` — load + interpolate the config (no binds)
//!   and report the resolved settings; exit non-zero on any error.
//! - `--healthcheck`        — connect to the configured metrics port, probe
//!   `GET /healthz`, and exit 0 on success / 1 on failure. Intended for the
//!   Docker `HEALTHCHECK` (the distroless runtime has no shell, so the daemon
//!   binary performs the probe itself).

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tpt_daemon::{Daemon, DaemonConfig};
use tracing::Level;
use tracing_subscriber::fmt;

#[derive(Default)]
struct Args {
    config: Option<String>,
    help: bool,
    version: bool,
    check: bool,
    healthcheck: bool,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--help" | "-h" => args.help = true,
            "--version" | "-V" => args.version = true,
            "--check" | "--validate-config" => args.check = true,
            "--healthcheck" => args.healthcheck = true,
            "--config" => args.config = iter.next(),
            s if let Some(rest) = s.strip_prefix("--config=") => {
                args.config = Some(rest.to_string())
            }
            _ => {}
        }
    }
    args
}

fn main() -> Result<()> {
    let args = parse_args();

    if args.help {
        print_usage();
        return Ok(());
    }
    if args.version {
        println!("tpt-daemon {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| "tpt-daemon.toml".to_string());
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config file `{config_path}`"))?;
    let mut cfg: DaemonConfig = toml::from_str(&text).context("parsing TOML config")?;
    // Interpolate ${ENV_VAR} even for --check / --healthcheck so resolved values
    // (and secret headers) are validated against the real effective config.
    cfg.interpolate_env();

    if args.check {
        return run_check(&cfg);
    }
    if args.healthcheck {
        return run_healthcheck(&cfg);
    }

    init_tracing(cfg.logging.level.trim());
    tracing::info!(config = %config_path, "starting tpt-daemon");

    let daemon = Daemon::new(cfg)?;
    let stop = daemon.stop_flag();

    ctrlc::set_handler({
        let stop = stop.clone();
        move || {
            tracing::info!("received shutdown signal");
            stop.store(true, Ordering::SeqCst);
        }
    })
    .context("failed to install Ctrl-C handler")?;

    let running = daemon.start();
    tracing::info!(
        udp = %running.udp,
        tcp = %running.tcp,
        metrics = %running.metrics,
        "tpt-daemon listening"
    );

    // Block until the shutdown flag is set.
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200));
    }

    running.stop();
    tracing::info!("tpt-daemon shut down");
    Ok(())
}

/// Load + interpolate the config and report the resolved settings without
/// binding any sockets. Header values are redacted.
fn run_check(cfg: &DaemonConfig) -> Result<()> {
    println!("config OK:");
    println!("  schema.path   = {}", cfg.schema.path);
    println!("  syslog.udp    = {}", cfg.syslog.udp.bind);
    println!("  syslog.tcp    = {}", cfg.syslog.tcp.bind);
    println!(
        "  syslog.tcp.framing = {} (max_frame_len={}, max_connections={})",
        match cfg.syslog.tcp.framing {
            tpt_daemon::config::FramingMode::Auto => "auto",
            tpt_daemon::config::FramingMode::Octet => "octet",
            tpt_daemon::config::FramingMode::Lf => "lf",
        },
        cfg.syslog.tcp.max_frame_len,
        cfg.syslog.tcp.max_connections,
    );
    println!(
        "  otlp.endpoint = {} (transport={:?}, require_tls={})",
        cfg.otlp.endpoint, cfg.otlp.transport, cfg.otlp.require_tls
    );
    print!("  otlp.headers  = {{");
    let mut first = true;
    for k in cfg.otlp.headers.keys() {
        if !first {
            print!(", ");
        }
        print!("{k}");
        first = false;
    }
    println!("}} (values redacted)");
    println!("  metrics.bind  = {}", cfg.metrics.bind);
    println!("  logging.level = {}", cfg.logging.level);
    Ok(())
}

/// Probe the configured metrics port over `GET /healthz`. Connects to
/// `127.0.0.1` on the configured port (the metrics listener binds an address
/// that may be `0.0.0.0`; loopback is always reachable locally).
fn run_healthcheck(cfg: &DaemonConfig) -> Result<()> {
    let addr =
        metrics_probe_addr(&cfg.metrics.bind).context("parsing metrics.bind for healthcheck")?;
    match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
        Ok(mut s) => {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let req = "GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            if s.write_all(req.as_bytes()).is_err() {
                anyhow::bail!("healthcheck: failed to send request");
            }
            let mut resp = Vec::new();
            let _ = s.read_to_end(&mut resp);
            let body = String::from_utf8_lossy(&resp);
            if body.contains("200") && body.contains("ok") {
                println!("healthcheck: OK");
                Ok(())
            } else {
                anyhow::bail!("healthcheck: unexpected response: {body}");
            }
        }
        Err(e) => anyhow::bail!("healthcheck: cannot connect to {addr}: {e}"),
    }
}

/// Map a metrics bind address (e.g. `0.0.0.0:9464`) to a loopback probe
/// address (`127.0.0.1:9464`).
fn metrics_probe_addr(bind: &str) -> Result<SocketAddr> {
    let sa: SocketAddr = bind.parse().context("invalid metrics.bind address")?;
    Ok(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, sa.port())))
}

fn print_usage() {
    println!(
        r#"tpt-daemon {} — unified syslog -> parse -> OTLP exporter

USAGE:
    tpt-daemon [FLAGS]

FLAGS:
    --config <path>          Path to the TOML config (default: tpt-daemon.toml)
    --check, --validate-config
                             Validate and print the resolved config, then exit
    --healthcheck           Probe GET /healthz on the metrics port, exit 0/1
    --version, -V           Print version and exit
    --help, -h              Print this help and exit"#,
        env!("CARGO_PKG_VERSION")
    );
}

fn init_tracing(level: &str) {
    let lvl = match level.to_ascii_lowercase().as_str() {
        "error" => Level::ERROR,
        "warn" | "warning" => Level::WARN,
        "info" => Level::INFO,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    };
    let _ = fmt().with_max_level(lvl).try_init();
}
