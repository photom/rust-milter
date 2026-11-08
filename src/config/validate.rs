//! The validation rules that run at startup and on every reload.

use std::collections::HashSet;
use std::fmt::Debug;
use std::time::Duration;

use super::ServerConfig;
use crate::ConfigError;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const MS: Duration = Duration::from_millis(1);
const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3600);
const DAY: Duration = Duration::from_secs(86_400);

/// File descriptors kept free beyond connections and listeners, so the
/// server can always accept, log, and close.
const SPARE_FDS: usize = 10;

/// Whether the platform has `SO_REUSEPORT`.
const REUSE_PORT_SUPPORTED: bool = cfg!(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly",
));

impl ServerConfig {
    /// Checks the configuration against the validation rules in
    /// `docs/reference/configuration.md`: every value in its range,
    /// `bind_addresses` not empty and without duplicates, no IPv4 address
    /// next to a dual-stack `[::]` on its port, `max_fds` and `max_memory`
    /// large enough, and `reuse_port` only where the platform supports it.
    ///
    /// The server runs the same checks when it is built and on every reload.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.bind_addresses.is_empty() {
            return Err(ConfigError::MissingBindAddress);
        }
        self.check_ranges()?;
        self.check_duplicates()?;
        self.check_dual_stack()?;
        self.check_limits()?;
        self.check_platform()
    }

    fn check_ranges(&self) -> Result<(), ConfigError> {
        for address in &self.bind_addresses {
            if address.port() == 0 {
                return Err(out_of_range("bind_addresses", address, "ports 1 to 65535"));
            }
        }
        in_range("backlog", self.backlog, 1, 65_535, "1 to 65535")?;
        in_range(
            "max_connections",
            self.max_connections,
            1,
            1_000_000,
            "1 to 1,000,000",
        )?;

        let timeouts = &self.timeouts;
        in_range(
            "timeouts.connect",
            timeouts.connect,
            MS,
            5 * MINUTE,
            "1 ms to 5 min",
        )?;
        in_range("timeouts.read", timeouts.read, MS, HOUR, "1 ms to 1 h")?;
        in_range("timeouts.write", timeouts.write, MS, HOUR, "1 ms to 1 h")?;
        in_range("timeouts.idle", timeouts.idle, SECOND, DAY, "1 s to 24 h")?;

        let socket = &self.socket_options;
        if let Some(size) = socket.recv_buffer_size {
            in_range(
                "socket_options.recv_buffer_size",
                size,
                KB,
                64 * MB,
                "1 KB to 64 MB",
            )?;
        }
        if let Some(size) = socket.send_buffer_size {
            in_range(
                "socket_options.send_buffer_size",
                size,
                KB,
                64 * MB,
                "1 KB to 64 MB",
            )?;
        }
        if let Some(keepalive) = &socket.keepalive {
            in_range(
                "socket_options.keepalive.time",
                keepalive.time,
                SECOND,
                DAY,
                "1 s to 24 h",
            )?;
            in_range(
                "socket_options.keepalive.interval",
                keepalive.interval,
                SECOND,
                HOUR,
                "1 s to 1 h",
            )?;
            in_range(
                "socket_options.keepalive.retries",
                keepalive.retries,
                1,
                30,
                "1 to 30",
            )?;
        }

        let shutdown = &self.shutdown;
        in_range(
            "shutdown.grace_period",
            shutdown.grace_period,
            Duration::ZERO,
            MINUTE,
            "0 s to 1 min",
        )?;
        in_range(
            "shutdown.drain_timeout",
            shutdown.drain_timeout,
            SECOND,
            5 * MINUTE,
            "1 s to 5 min",
        )?;

        // The lower bound holds one full default-size packet: 4 length
        // bytes, 1 command byte, and 65,535 data bytes.
        in_range(
            "resource_limits.per_connection_buffer",
            self.resource_limits.per_connection_buffer,
            65_540,
            16 * MB,
            "65,540 bytes to 16 MB",
        )
    }

    fn check_duplicates(&self) -> Result<(), ConfigError> {
        let mut seen = HashSet::new();
        for address in &self.bind_addresses {
            if !seen.insert(address) {
                return Err(ConfigError::DuplicateBindAddress(*address));
            }
        }
        Ok(())
    }

    /// A dual-stack `[::]` listener takes the IPv4 connections on its port,
    /// so an IPv4 listener on the same port cannot bind next to it.
    fn check_dual_stack(&self) -> Result<(), ConfigError> {
        if self.socket_options.ipv6_only {
            return Ok(());
        }
        for v6 in &self.bind_addresses {
            if v6.ip() != std::net::Ipv6Addr::UNSPECIFIED {
                continue;
            }
            for v4 in &self.bind_addresses {
                if v4.is_ipv4() && v4.port() == v6.port() {
                    return Err(ConfigError::Inconsistent {
                        rule: "ipv6_only",
                        detail: format!(
                            "{v6} also receives IPv4 connections while ipv6_only is false, \
                             so {v4} cannot bind; set ipv6_only to true or remove {v4}"
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_limits(&self) -> Result<(), ConfigError> {
        let limits = &self.resource_limits;
        if let Some(max_fds) = limits.max_fds {
            let needed = self.max_connections + self.bind_addresses.len() + SPARE_FDS;
            if max_fds < needed {
                return Err(ConfigError::Inconsistent {
                    rule: "max_fds",
                    detail: format!(
                        "max_fds is {max_fds}; it must be at least max_connections + \
                         listeners + {SPARE_FDS} = {needed}"
                    ),
                });
            }
        }
        if let Some(max_memory) = limits.max_memory {
            if max_memory < limits.per_connection_buffer {
                return Err(ConfigError::Inconsistent {
                    rule: "max_memory",
                    detail: format!(
                        "max_memory is {max_memory}; it must be at least \
                         per_connection_buffer = {}",
                        limits.per_connection_buffer
                    ),
                });
            }
        }
        Ok(())
    }

    fn check_platform(&self) -> Result<(), ConfigError> {
        if self.socket_options.reuse_port && !REUSE_PORT_SUPPORTED {
            return Err(ConfigError::Unsupported {
                field: "socket_options.reuse_port",
                reason: "the platform has no SO_REUSEPORT",
            });
        }
        Ok(())
    }
}

/// Fails with [`ConfigError::OutOfRange`] unless `min <= value <= max`.
fn in_range<T: PartialOrd + Debug>(
    field: &'static str,
    value: T,
    min: T,
    max: T,
    allowed: &'static str,
) -> Result<(), ConfigError> {
    if value < min || value > max {
        return Err(out_of_range(field, value, allowed));
    }
    Ok(())
}

fn out_of_range(field: &'static str, value: impl Debug, allowed: &'static str) -> ConfigError {
    ConfigError::OutOfRange {
        field,
        value: format!("{value:?}"),
        allowed,
    }
}
