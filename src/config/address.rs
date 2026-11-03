//! Parsing bind addresses from text.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::ServerConfig;
use crate::AddressError;

impl ServerConfig {
    /// Parses a bind address from text.
    ///
    /// Accepts `host:port` with an IPv4 address or a bracketed IPv6 address,
    /// such as `127.0.0.1:8891` or `[::1]:8891`, and libmilter's forms
    /// `inet:port@host` and `inet6:port@host`. The host must be an IP address;
    /// no name is looked up. `unix:` and `local:` addresses are rejected,
    /// because rust-milter listens on TCP only. The port must be 1 to 65535.
    pub fn parse_bind_address(text: &str) -> Result<SocketAddr, AddressError> {
        if let Some(rest) = text.strip_prefix("inet:") {
            let (port, host) = split_libmilter(rest)?;
            let host: Ipv4Addr = host.parse().map_err(|_| AddressError::InvalidHost)?;
            return Ok(SocketAddr::new(IpAddr::V4(host), parse_port(port)?));
        }
        if let Some(rest) = text.strip_prefix("inet6:") {
            let (port, host) = split_libmilter(rest)?;
            let host: Ipv6Addr = host.parse().map_err(|_| AddressError::InvalidHost)?;
            return Ok(SocketAddr::new(IpAddr::V6(host), parse_port(port)?));
        }
        if text.starts_with("unix:") || text.starts_with("local:") {
            return Err(AddressError::InvalidFormat);
        }

        let (host, port) = text.rsplit_once(':').ok_or(AddressError::InvalidFormat)?;
        let host = match host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            Some(v6) => v6.parse().map(IpAddr::V6),
            None => host.parse().map(IpAddr::V4),
        }
        .map_err(|_| AddressError::InvalidHost)?;
        Ok(SocketAddr::new(host, parse_port(port)?))
    }
}

/// Splits libmilter's `port@host`.
fn split_libmilter(text: &str) -> Result<(&str, &str), AddressError> {
    text.split_once('@').ok_or(AddressError::InvalidFormat)
}

/// Parses a port from 1 to 65535.
fn parse_port(text: &str) -> Result<u16, AddressError> {
    match text.parse::<u16>() {
        Ok(0) | Err(_) => Err(AddressError::InvalidPort),
        Ok(port) => Ok(port),
    }
}
