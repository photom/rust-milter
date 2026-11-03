//! Parsing bind addresses from text.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use super::ServerConfig;
use crate::AddressError;

impl ServerConfig {
    /// Parses a bind address from text.
    ///
    /// Accepts `host:port` with an IPv4 address or a bracketed IPv6 address,
    /// such as `127.0.0.1:8891` or `[::1]:8891`, and libmilter's forms
    /// `inet:port@host` and `inet6:port@host`. Leaving out `@host` means
    /// every address of the family, and an IPv6 host may carry a numeric
    /// scope ID, as in `[fe80::1%2]:8891`. The host must be an IP address; no
    /// name is looked up. `unix:` and `local:` addresses are rejected, because
    /// rust-milter listens on TCP only. The port must be 1 to 65535.
    pub fn parse_bind_address(text: &str) -> Result<SocketAddr, AddressError> {
        if let Some(rest) = text.strip_prefix("inet:") {
            let (port, host) = split_libmilter(rest)?;
            let host = match host {
                None => Ipv4Addr::UNSPECIFIED,
                Some(host) => host.parse().map_err(|_| AddressError::InvalidHost)?,
            };
            return Ok(SocketAddr::new(IpAddr::V4(host), parse_port(port)?));
        }
        if let Some(rest) = text.strip_prefix("inet6:") {
            let (port, host) = split_libmilter(rest)?;
            let (host, scope_id) = match host {
                None => (Ipv6Addr::UNSPECIFIED, 0),
                Some(host) => parse_ipv6(host)?,
            };
            return Ok(SocketAddrV6::new(host, parse_port(port)?, 0, scope_id).into());
        }
        if text.starts_with("unix:") || text.starts_with("local:") {
            return Err(AddressError::InvalidFormat);
        }

        let (host, port) = text.rsplit_once(':').ok_or(AddressError::InvalidFormat)?;
        if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            let (host, scope_id) = parse_ipv6(v6)?;
            return Ok(SocketAddrV6::new(host, parse_port(port)?, 0, scope_id).into());
        }
        let host: Ipv4Addr = host.parse().map_err(|_| AddressError::InvalidHost)?;
        Ok(SocketAddr::new(IpAddr::V4(host), parse_port(port)?))
    }
}

/// Splits libmilter's `port@host`. Without `@host`, libmilter binds every
/// address of the family. A colon there means another form, such as
/// Postfix's `inet:host:port`, so it is reported as a format error rather
/// than as a bad port.
fn split_libmilter(text: &str) -> Result<(&str, Option<&str>), AddressError> {
    match text.split_once('@') {
        Some((port, host)) => Ok((port, Some(host))),
        None if text.contains(':') => Err(AddressError::InvalidFormat),
        None => Ok((text, None)),
    }
}

/// Parses an IPv6 address with an optional numeric scope ID, `addr%id`.
fn parse_ipv6(text: &str) -> Result<(Ipv6Addr, u32), AddressError> {
    let (address, scope_id) = match text.split_once('%') {
        Some((address, scope)) => (
            address,
            scope.parse().map_err(|_| AddressError::InvalidHost)?,
        ),
        None => (text, 0),
    };
    let address = address.parse().map_err(|_| AddressError::InvalidHost)?;
    Ok((address, scope_id))
}

/// Parses a port from 1 to 65535.
fn parse_port(text: &str) -> Result<u16, AddressError> {
    match text.parse::<u16>() {
        Ok(0) | Err(_) => Err(AddressError::InvalidPort),
        Ok(port) => Ok(port),
    }
}
