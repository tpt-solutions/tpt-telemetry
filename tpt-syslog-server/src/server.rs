//! Syslog server: UDP + TCP receivers feeding a bounded ring buffer.

use crate::framing::TcpFraming;
use crate::message::{Framing, Message, Transport};
use crate::stats::{Stats, StatsSnapshot};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::{DEFAULT_READ_TIMEOUT_MS, DEFAULT_RING_CAPACITY};

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
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            udp_bind: "127.0.0.1:0".parse().unwrap(),
            tcp_bind: "127.0.0.1:0".parse().unwrap(),
            ring_capacity: DEFAULT_RING_CAPACITY,
            read_timeout_ms: DEFAULT_READ_TIMEOUT_MS,
            tcp_framing: TcpFraming::Auto,
        }
    }
}

impl ServerConfig {
    /// Bind both listeners to the given ports on localhost.
    pub fn localhost(udp_port: u16, tcp_port: u16) -> Self {
        let mut c = ServerConfig::default();
        c.udp_bind = SocketAddr::from(([127, 0, 0, 1], udp_port));
        c.tcp_bind = SocketAddr::from(([127, 0, 0, 1], tcp_port));
        c
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
            handles.push(thread::spawn(move || {
                tcp_accept_loop(tcp, tx, &stats, &stop, framing, read_timeout);
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

fn tcp_accept_loop(
    listener: TcpListener,
    tx: SyncSender<Message>,
    stats: &Arc<Stats>,
    stop: &Arc<AtomicBool>,
    framing: TcpFraming,
    read_timeout: u64,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(read_timeout)));
                let tx = tx.clone();
                let stats = Arc::clone(stats);
                let stop = Arc::clone(stop);
                thread::spawn(move || {
                    tcp_conn_loop(stream, addr, tx, &stats, &stop, framing);
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
) {
    let mut decoder = crate::framing::TcpDecoder::new(framing);
    let mut buf = vec![0u8; 65536];
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(16);
    while !stop.load(Ordering::SeqCst) {
        match stream.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                decoder.push(&buf[..n], &mut frames);
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
    decoder.flush(&mut frames);
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
    let _ = stream.flush();
}

/// Push a message into the ring buffer; drop (and count) if it is full.
fn deliver(tx: &SyncSender<Message>, stats: &Arc<Stats>, msg: Message) {
    match tx.try_send(msg) {
        Ok(()) => {
            stats.delivered.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Full(_)) => {
            stats.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Disconnected(_)) => {
            stats.dropped.fetch_add(1, Ordering::Relaxed);
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
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    // Minimal decode of sockaddr_in / sockaddr_in6.
    if buf.len() >= 4 && (buf[1] == libc::AF_INET as u8) {
        let port = u16::from_be_bytes([buf[2], buf[3]]);
        let ip = Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
        SocketAddr::from((ip, port))
    } else if buf.len() >= 8 && (buf[1] == libc::AF_INET6 as u8) {
        let port = u16::from_be_bytes([buf[2], buf[3]]);
        let mut oct = [0u8; 16];
        oct.copy_from_slice(&buf[8..24]);
        SocketAddr::from((Ipv6Addr::from(oct), port))
    } else {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))
    }
}
