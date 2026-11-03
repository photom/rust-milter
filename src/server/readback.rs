//! Reading an accepted socket's options back, for rust-milter's own tests
//! (feature `test-support`). See "Socket option readback" in
//! `docs/reference/test-environment.md`.

use std::fmt;
use std::io;
use std::sync::Arc;

use socket2::Socket;

use super::ConnectionInfo;
use crate::KeepaliveConfig;

/// A duplicate handle of an accepted socket. Reading options through it
/// reads the options of the socket the connection runs on.
#[derive(Clone)]
pub(crate) struct Probe(Arc<Socket>);

impl Probe {
    pub(crate) fn new(stream: &tokio::net::TcpStream) -> io::Result<Probe> {
        Ok(Probe(Arc::new(socket2::SockRef::from(stream).try_clone()?)))
    }
}

impl fmt::Debug for Probe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Probe")
    }
}

impl PartialEq for Probe {
    fn eq(&self, other: &Probe) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Probe {}

/// The options of an accepted socket, as the operating system reports them.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketReadback {
    /// `TCP_NODELAY`.
    pub tcp_nodelay: bool,
    /// Keepalive with its time, interval, and retries; `None` when off.
    pub keepalive: Option<KeepaliveConfig>,
    /// `SO_RCVBUF`.
    pub recv_buffer_size: usize,
    /// `SO_SNDBUF`.
    pub send_buffer_size: usize,
}

impl ConnectionInfo {
    /// Reads the options of the socket this connection runs on.
    #[doc(hidden)]
    pub fn read_socket_options(&self) -> io::Result<SocketReadback> {
        let socket = &self.probe.0;
        let keepalive = if socket.keepalive()? {
            Some(KeepaliveConfig {
                time: socket.tcp_keepalive_time()?,
                interval: socket.tcp_keepalive_interval()?,
                retries: socket.tcp_keepalive_retries()?,
            })
        } else {
            None
        };
        Ok(SocketReadback {
            tcp_nodelay: socket.tcp_nodelay()?,
            keepalive,
            recv_buffer_size: socket.recv_buffer_size()?,
            send_buffer_size: socket.send_buffer_size()?,
        })
    }
}
