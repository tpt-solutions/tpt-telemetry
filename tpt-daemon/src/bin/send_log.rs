//! `tpt-send-log` — fire sample syslog lines at a running `tpt-daemon`.
//!
//! Pairs with `examples/` (e.g. `examples/schemas/cisco_asa.tpt-log`). Hand-rolled
//! argument parsing (no `clap`, matching the rest of the workspace tooling).
//!
//! USAGE:
//!   tpt-send-log [--udp <addr> | --tcp <addr>] [--message <text> | --file <path>]
//!               [--repeat N] [--interval-ms N]
//!
//! Defaults: `--udp 127.0.0.1:514` and a sample Cisco ASA message.

use std::io::{self, Read};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

fn main() {
    if let Err(e) = run() {
        eprintln!("tpt-send-log: {e}");
        std::process::exit(1);
    }
}

struct Opts {
    target: Target,
    messages: Vec<String>,
    repeat: usize,
    interval_ms: u64,
}

enum Target {
    Udp(String),
    Tcp(String),
}

fn run() -> Result<(), String> {
    let mut opts = Opts {
        target: Target::Udp("127.0.0.1:514".to_string()),
        messages: vec!["%ASA-6-302013: Built inbound TCP connection".to_string()],
        repeat: 1,
        interval_ms: 0,
    };

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--udp" => {
                opts.target = Target::Udp(arg(&mut args, "--udp")?);
            }
            "--tcp" => {
                opts.target = Target::Tcp(arg(&mut args, "--tcp")?);
            }
            "--message" => {
                opts.messages = vec![arg(&mut args, "--message")?];
            }
            "--file" => {
                let path = arg(&mut args, "--file")?;
                opts.messages = read_lines(&path)?;
            }
            "--repeat" => {
                opts.repeat = arg(&mut args, "--repeat")?
                    .parse()
                    .map_err(|_| "invalid --repeat (expected integer)".to_string())?;
            }
            "--interval-ms" => {
                opts.interval_ms = arg(&mut args, "--interval-ms")?
                    .parse()
                    .map_err(|_| "invalid --interval-ms (expected integer)".to_string())?;
            }
            "--stdin" => {
                opts.messages = read_stdin_lines();
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    if opts.messages.is_empty() {
        return Err("no messages to send".to_string());
    }

    match &opts.target {
        Target::Udp(addr) => {
            let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
            let resolved = resolve(addr)?;
            for _ in 0..opts.repeat {
                for m in &opts.messages {
                    sock.send_to(m.as_bytes(), resolved)
                        .map_err(|e| format!("send_to {resolved}: {e}"))?;
                    eprintln!("sent (udp) -> {resolved}: {m}");
                    sleep(opts.interval_ms);
                }
            }
        }
        Target::Tcp(addr) => {
            let resolved = resolve(addr)?;
            let mut stream =
                TcpStream::connect(resolved).map_err(|e| format!("connect {resolved}: {e}"))?;
            for _ in 0..opts.repeat {
                for m in &opts.messages {
                    use std::io::Write;
                    stream
                        .write_all(m.as_bytes())
                        .and_then(|_| stream.write_all(b"\n"))
                        .map_err(|e| format!("write {resolved}: {e}"))?;
                    eprintln!("sent (tcp) -> {resolved}: {m}");
                    sleep(opts.interval_ms);
                }
            }
        }
    }
    Ok(())
}

fn arg<I: Iterator<Item = String>>(iter: &mut I, name: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("missing value for {name}"))
}

/// Resolve a host:port once so repeated sends target a stable address.
fn resolve(addr: &str) -> Result<std::net::SocketAddr, String> {
    addr.to_socket_addrs()
        .map_err(|e| format!("resolve {addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address resolved for {addr}"))
}

fn read_lines(path: &str) -> Result<Vec<String>, String> {
    let data = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    Ok(data.lines().map(|l| l.to_string()).collect())
}

fn read_stdin_lines() -> Vec<String> {
    let mut buf = String::new();
    let _ = io::stdin().read_to_string(&mut buf);
    buf.lines().map(|l| l.to_string()).collect()
}

fn sleep(ms: u64) {
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

fn print_help() {
    println!(
        r#"tpt-send-log — fire sample syslog lines at a running tpt-daemon

USAGE:
    tpt-send-log [FLAGS]

FLAGS:
    --udp <addr>        Send via UDP to <addr> (default: 127.0.0.1:514)
    --tcp <addr>        Send via TCP (RFC3164 LF framing) to <addr>
    --message <text>    Single message to send
    --file <path>       Send each line of <path> as a message
    --stdin             Read messages, one per line, from stdin
    --repeat <N>        Send the message set N times (default: 1)
    --interval-ms <N>   Wait N ms between sends (default: 0)
    --help, -h          Print this help"#
    );
}
