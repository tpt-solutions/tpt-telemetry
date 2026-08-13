//! Syslog server: UDP + TCP receivers feeding a bounded ring buffer.

use crate::framing::{FrameError, TcpFraming};
use crate::message::{Framing, Message, Transport};
use crate::stats::{Stats, StatsSnapshot};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::{
    DEFAULT_MAX_CONNECTIONS, DEFAULT_MAX_FRAME_LEN, DEFAULT_READ_TIMEOUT_MS, DEFAULT_RING_CAPACITY,
};

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// UDP bind address (RFC3164 / RFC5424-over-UDP datagrams).
    pub udp_bind: SocketAddr,
    /// TCP bind address (RFC3164 LF / RFC5424 octet-counting framing).
    pub tcp_bind: SocketAddr,
    /// Ring-buffer capacity (in messages) used for backpressure.
    pub ring_capacity: usize,
    /// Socket read timeout used to periodically re-check the shutdown flag.
    pub read_timeout_ms: u64,
    /// TCP framing mode.
    pub tcp_framing: TcpFraming,
    /// Maximum number of concurrent TCP connections. Connections beyond this
    /// cap are accepted then immediately closed and counted as rejected.
    pub max_connections: usize,
    /// Per-frame size ceiling (bytes) enforced by the TCP framing decoder.
    pub max_frame_len: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            udp_bind: "127.0.0.1:0".parse().unwrap(),
            tcp_bind: "127.0.0.1:0".parse().unwrap(),
            ring_capacity: DEFAULT_RING_CAPACITY,
            read_timeout_ms: DEFAULT_READ_TIMEOUT_MS,
            tcp_framing: TcpFraming::Auto,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            max_frame_len: DEFAULT_MAX_FRAME_LEN,
        }
    }
}

impl ServerConfig {
    /// Bind both listeners to the given ports on localhost.
    pub fn localhost(udp_port: u16, tcp_port: u16) -> Self {
        ServerConfig {
            udp_bind: SocketAddr::from(([127, 0, 0, 1], udp_port)),
            tcp_bind: SocketAddr::from(([127, 0, 0, 1], tcp_port)),
            ..Default::default()
        }
    }
}

/// A running syslog server.
pub struct SyslogServer {
    config: ServerConfig,
    stop: Arc<AtomicBool>,
    stats: Arc<Stats>,
    handles: Vec<JoinHandle<()>>,
    rx: Receiver<Message>,
    local_udp: SocketAddr,
    local_tcp: SocketAddr,
}

impl SyslogServer {
    /// Bind the listeners and start the receiver worker threads.
    pub fn new(config: ServerConfig) -> io::Result<Self> {
        let udp = UdpSocket::bind(config.udp_bind)?;
        let local_udp = udp.local_addr()?;
        udp.set_read_timeout(Some(Duration::from_millis(config.read_timeout_ms)))?;
        #[cfg(target_os = "linux")]
        enable_rxq_ovfl(udp.as_raw_fd());

        let tcp = TcpListener::bind(config.tcp_bind)?;
        let local_tcp = tcp.local_addr()?;
        tcp.set_nonblocking(true)?;

        let (tx, rx) = sync_channel(config.ring_capacity.max(1));
        let stop = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Stats::new());

        let mut handles = Vec::new();

        // UDP receiver thread.
        {
            let tx = tx.clone();
            let stop = Arc::clone(&stop);
            let stats = Arc::clone(&stats);
            let read_timeout = config.read_timeout_ms;
            handles.push(thread::spawn(move || {
                udp_recv_loop(udp, tx, &stats, &stop, read_timeout);
            }));
        }

        // TCP acceptor thread.
        {
            let tx = tx.clone();
            let stop = Arc::clone(&stop);
            let stats = Arc::clone(&stats);
            let framing = config.tcp_framing;
            let read_timeout = config.read_timeout_ms;
            let max_connections = config.max_connections;
            let max_frame_len = config.max_frame_len;
            let live = Arc::new(AtomicUsize::new(0));
            handles.push(thread::spawn(move || {
                tcp_accept_loop(
                    tcp,
                    tx,
                    &stats,
                    &stop,
                    framing,
                    read_timeout,
                    max_connections,
                    max_frame_len,
                    live,
                );
            }));
        }

        Ok(SyslogServer {
            config,
            stop,
            stats,
            handles,
            rx,
            local_udp,
            local_tcp,
        })
    }

    /// The channel of received messages (bounded ring buffer).
    pub fn messages(&self) -> &Receiver<Message> {
        &self.rx
    }

    /// Block up to `timeout` waiting for the next message.
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Message, std::sync::mpsc::RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }

    /// Non-blocking receive.
    pub fn try_recv(&self) -> Result<Message, std::sync::mpsc::TryRecvError> {
        self.rx.try_recv()
    }

    /// Local UDP bind address (useful when bound to port 0).
    pub fn local_udp_addr(&self) -> SocketAddr {
        self.local_udp
    }

    /// Local TCP bind address (useful when bound to port 0).
    pub fn local_tcp_addr(&self) -> SocketAddr {
        self.local_tcp
    }

    /// The configuration the server was started with.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Snapshot of delivery / drop / overflow statistics.
    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    /// Signal shutdown and join all worker threads.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

impl Drop for SyslogServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Receiver loops
// ---------------------------------------------------------------------------

fn udp_recv_loop(
    socket: UdpSocket,
    tx: SyncSender<Message>,
    stats: &Arc<Stats>,
    stop: &Arc<AtomicBool>,
    read_timeout: u64,
) {
    #[cfg(target_os = "linux")]
    {
        let fd = socket.as_raw_fd();
        udp_recv_loop_linux(fd, tx, stats, stop);
        return;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut buf = vec![0u8; 65507];
        while !stop.load(Ordering::SeqCst) {
            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let msg = Message {
                        transport: Transport::Udp,
                        remote: addr,
                        framing: Framing::Datagram,
                        payload: buf[..n].to_vec(),
                        received_at: SystemTime::now(),
                    };
                    deliver(&tx, stats, msg);
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    thread::sleep(Duration::from_millis(read_timeout));
                }
                Err(_) => break,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tcp_accept_loop(
    listener: TcpListener,
    tx: SyncSender<Message>,
    stats: &Arc<Stats>,
    stop: &Arc<AtomicBool>,
    framing: TcpFraming,
    read_timeout: u64,
    max_connections: usize,
    max_frame_len: usize,
    live: Arc<AtomicUsize>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, addr)) => {
                // Enforce the connection cap. We bump first and reject if we have
                // already reached the limit, closing the surplus connection.
                let cur = live.fetch_add(1, Ordering::SeqCst);
                if cur >= max_connections {
                    live.fetch_sub(1, Ordering::SeqCst);
                    stats.rejected_connections.fetch_add(1, Ordering::Relaxed);
                    drop(stream);
                    continue;
                }
                let tx = tx.clone();
                let stats = Arc::clone(stats);
                let stop = Arc::clone(stop);
                let live = Arc::clone(&live);
                thread::spawn(move || {
                    tcp_conn_loop(stream, addr, tx, &stats, &stop, framing, max_frame_len);
                    live.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(read_timeout));
            }
            Err(_) => break,
        }
    }
}

fn tcp_conn_loop(
    mut stream: TcpStream,
    addr: SocketAddr,
    tx: SyncSender<Message>,
    stats: &Arc<Stats>,
    stop: &Arc<AtomicBool>,
    framing: TcpFraming,
    max_frame_len: usize,
) {
    let mut decoder = crate::framing::TcpDecoder::new(framing, max_frame_len);
    let mut buf = vec![0u8; 65536];
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(16);
    while !stop.load(Ordering::SeqCst) {
        match stream.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                match decoder.push(&buf[..n], &mut frames) {
                    Ok(()) => {}
                    Err(FrameError::FrameTooLarge { .. }) => {
                        // The peer sent a frame that violates the size ceiling.
                        // Drop the connection rather than buffering attacker
                        // controlled bytes indefinitely.
                        break;
                    }
                }
                for f in frames.drain(..) {
                    let msg = Message {
                        transport: Transport::Tcp,
                        remote: addr,
                        framing: match framing {
                            TcpFraming::OctetCounting => Framing::Rfc5424OctetCounting,
                            _ => Framing::Rfc3164Lf,
                        },
                        payload: f,
                        received_at: SystemTime::now(),
                    };
                    deliver(&tx, stats, msg);
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
            }
            Err(_) => break,
        }
    }
    // Flush any trailing frame on close.
    if decoder.flush(&mut frames).is_ok() {
        for f in frames.drain(..) {
            let msg = Message {
                transport: Transport::Tcp,
                remote: addr,
                framing: match framing {
                    TcpFraming::OctetCounting => Framing::Rfc5424OctetCounting,
                    _ => Framing::Rfc3164Lf,
                },
                payload: f,
                received_at: SystemTime::now(),
            };
            deliver(&tx, stats, msg);
        }
    }
    let _ = stream.flush();
}

/// Push a message into the ring buffer; drop (and count) if it is full.
fn deliver(tx: &SyncSender<Message>, stats: &Arc<Stats>, msg: Message) {
    match tx.try_send(msg) {
        Ok(()) => {
            stats.delivered.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Full(_)) => {
            stats.dropped_full.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Disconnected(_)) => {
            stats.dropped_disconnected.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Linux kernel overflow integration (SO_RXQ_OVFL)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn enable_rxq_ovfl(fd: std::os::unix::io::RawFd) {
    unsafe {
        let val: libc::c_int = 1;
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RXQ_OVFL,
            &val as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }
}

#[cfg(target_os = "linux")]
fn udp_recv_loop_linux(
    fd: std::os::unix::io::RawFd,
    tx: SyncSender<Message>,
    stats: &Arc<Stats>,
    stop: &Arc<AtomicBool>,
) {
    use libc::{
        c_int, c_void, cmsghdr, iovec, msghdr, recvmsg, socklen_t, CMSG_DATA, CMSG_FIRSTHDR,
        CMSG_NXTHDR, MSG_DONTWAIT, SCM_RXQ_OVFL, SOL_SOCKET,
    };
    use std::os::unix::io::AsRawFd;

    let mut buf = vec![0u8; 65507];
    let mut cmsg = vec![0u8; 128];
    let mut name = vec![0u8; 128];
    let stats = Arc::clone(stats);

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let mut iov = iovec {
            iov_base: buf.as_mut_ptr() as *mut c_void,
            iov_len: buf.len(),
        };
        let mut hdr = msghdr {
            msg_name: name.as_mut_ptr() as *mut c_void,
            msg_namelen: name.len() as socklen_t,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: cmsg.as_mut_ptr() as *mut c_void,
            msg_controllen: cmsg.len() as libc::size_t,
            msg_flags: 0,
        };
        let n = unsafe { recvmsg(fd, &mut hdr, MSG_DONTWAIT) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock {
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            continue;
        }
        // Walk control messages for SCM_RXQ_OVFL.
        let mut cmsg_ptr = unsafe { CMSG_FIRSTHDR(&hdr) };
        while !cmsg_ptr.is_null() {
            let cm = unsafe { &*cmsg_ptr };
            if cm.cmsg_level == SOL_SOCKET && cm.cmsg_type == SCM_RXQ_OVFL {
                let v = unsafe { *(CMSG_DATA(cm) as *const u32) };
                stats.kernel_overflow.store(v as u64, Ordering::Relaxed);
            }
            cmsg_ptr = unsafe { CMSG_NXTHDR(&hdr, cmsg_ptr) };
        }
        // Resolve remote address from msg_name.
        let remote = sockaddr_to_addr(&name[..hdr.msg_namelen as usize]);
        let payload = buf[..n as usize].to_vec();
        let msg = Message {
            transport: Transport::Udp,
            remote,
            framing: Framing::Datagram,
            payload,
            received_at: SystemTime::now(),
        };
        deliver(&tx, stats, msg);
    }
}

#[cfg(target_os = "linux")]
fn sockaddr_to_addr(buf: &[u8]) -> SocketAddr {
    // The family byte is at offset 1 of the kernel sockaddr; the rest is the
    // address body. A short/unknown buffer falls back to an unspecified address
    // rather than reading past the end of `buf`.
    decode_sockaddr(buf.get(1).copied().unwrap_or(0), buf)
        .unwrap_or_else(|| SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, 0)))
}

// `sa_family` byte values. On Linux we mirror the kernel constants; elsewhere
// (where `libc` does not expose socket families) we hard-code the conventional
// values so the platform-independent decode path can still be unit-tested.
#[cfg(target_os = "linux")]
const SOCKADDR_AF_INET: u8 = libc::AF_INET as u8;
#[cfg(target_os = "linux")]
const SOCKADDR_AF_INET6: u8 = libc::AF_INET6 as u8;
#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
const SOCKADDR_AF_INET: u8 = 2;
#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
const SOCKADDR_AF_INET6: u8 = 23;

/// Platform-independent decode of a raw `sockaddr_*` body (`buf` starts at the
/// family byte). Returns `None` if the buffer is too short for the declared
/// address family, guarding against out-of-bounds reads in `sockaddr_to_addr`.
#[allow(dead_code)]
fn decode_sockaddr(family: u8, buf: &[u8]) -> Option<SocketAddr> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    if family == SOCKADDR_AF_INET {
        // sockaddr_in: port at 2..4, IPv4 at 4..8.
        if buf.len() < 8 {
            return None;
        }
        let port = u16::from_be_bytes([buf[2], buf[3]]);
        let ip = Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
        Some(SocketAddr::from((ip, port)))
    } else if family == SOCKADDR_AF_INET6 {
        // sockaddr_in6: port at 2..4, IPv6 at 8..24.
        if buf.len() < 24 {
            return None;
        }
        let port = u16::from_be_bytes([buf[2], buf[3]]);
        let mut oct = [0u8; 16];
        oct.copy_from_slice(&buf[8..24]);
        Some(SocketAddr::from((Ipv6Addr::from(oct), port)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn decode_sockaddr_rejects_short_ipv4() {
        // sockaddr_in needs >= 8 bytes after the family byte.
        let buf = [0u8; 4];
        assert!(decode_sockaddr(SOCKADDR_AF_INET, &buf).is_none());
    }

    #[test]
    fn decode_sockaddr_rejects_short_ipv6() {
        // sockaddr_in6 needs >= 24 bytes after the family byte.
        let buf = [0u8; 16];
        assert!(decode_sockaddr(SOCKADDR_AF_INET6, &buf).is_none());
    }

    #[test]
    fn decode_sockaddr_decodes_ipv4() {
        let mut buf = [0u8; 8];
        buf[0] = SOCKADDR_AF_INET;
        buf[2..4].copy_from_slice(&8080u16.to_be_bytes());
        buf[4..8].copy_from_slice(&[127, 0, 0, 1]);
        let sa = decode_sockaddr(SOCKADDR_AF_INET, &buf).unwrap();
        assert_eq!(sa, SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), 8080)));
    }

    #[test]
    fn decode_sockaddr_decodes_ipv6() {
        let mut buf = [0u8; 24];
        buf[0] = SOCKADDR_AF_INET6;
        buf[2..4].copy_from_slice(&9090u16.to_be_bytes());
        buf[8..24].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        let sa = decode_sockaddr(SOCKADDR_AF_INET6, &buf).unwrap();
        assert_eq!(sa, SocketAddr::from((Ipv6Addr::LOCALHOST, 9090)));
    }

    #[test]
    fn decode_sockaddr_unknown_family_is_none() {
        let buf = [0u8; 32];
        assert!(decode_sockaddr(0x7f, &buf).is_none());
    }
}
