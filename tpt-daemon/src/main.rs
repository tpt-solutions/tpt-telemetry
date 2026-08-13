//! `tpt-daemon` binary entry point.

use anyhow::{Context, Result};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tpt_daemon::{Daemon, DaemonConfig};
use tracing::Level;
use tracing_subscriber::fmt;

fn main() -> Result<()> {
    let config_path = parse_config_arg().unwrap_or_else(|| "tpt-daemon.toml".to_string());
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config file `{config_path}`"))?;
    let mut cfg: DaemonConfig = toml::from_str(&text).context("parsing TOML config")?;
    cfg.interpolate_env();

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

fn parse_config_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" {
            return args.next();
        }
        if let Some(rest) = a.strip_prefix("--config=") {
            return Some(rest.to_string());
        }
    }
    None
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
