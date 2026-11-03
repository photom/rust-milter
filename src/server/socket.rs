//! Creating listeners and setting the options of accepted sockets.

use std::io;
use std::net::SocketAddr;

use socket2::{Domain, Protocol, SockRef, Socket, TcpKeepalive, Type};
use tokio::net::{TcpListener, TcpStream};

use crate::SocketOptions;

/// A listener socket for `addr` with the listener options set, not yet
/// bound.
fn listener_socket(addr: SocketAddr, options: &SocketOptions) -> io::Result<Socket> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(options.reuse_addr)?;
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    ))]
    if options.reuse_port {
        socket.set_reuse_port(true)?;
    }
    if addr.is_ipv6() {
        socket.set_only_v6(options.ipv6_only)?;
    }
    Ok(socket)
}

/// Binds and listens on `addr`.
pub(super) fn listen(
    addr: SocketAddr,
    options: &SocketOptions,
    backlog: u32,
) -> io::Result<TcpListener> {
    let socket = listener_socket(addr, options)?;
    socket.bind(&addr.into())?;
    // The backlog is validated to at most 65,535, so it fits in an i32.
    socket.listen(i32::try_from(backlog).unwrap_or(i32::MAX))?;
    socket.set_nonblocking(true)?;
    TcpListener::from_std(socket.into())
}

/// Sets the options of an accepted connection, before its first read.
pub(super) fn configure(stream: &TcpStream, options: &SocketOptions) -> io::Result<()> {
    let socket = SockRef::from(stream);
    socket.set_tcp_nodelay(options.tcp_nodelay)?;
    match &options.keepalive {
        Some(keepalive) => {
            let params = TcpKeepalive::new()
                .with_time(keepalive.time)
                .with_interval(keepalive.interval)
                .with_retries(keepalive.retries);
            socket.set_tcp_keepalive(&params)?;
        }
        None => socket.set_keepalive(false)?,
    }
    if let Some(size) = options.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }
    if let Some(size) = options.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srv_004_sets_ipv6_only_on_ipv6_listeners() {
        let addr: SocketAddr = "[::1]:8891".parse().unwrap();
        for ipv6_only in [true, false] {
            let options = SocketOptions {
                ipv6_only,
                ..SocketOptions::default()
            };
            let socket = match listener_socket(addr, &options) {
                Ok(socket) => socket,
                Err(error) => {
                    eprintln!("skipped: no IPv6 sockets here: {error}");
                    return;
                }
            };
            assert_eq!(socket.only_v6().unwrap(), ipv6_only);
        }
    }
}
