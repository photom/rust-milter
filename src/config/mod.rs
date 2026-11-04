//! Server configuration: plain data types with their defaults, and the
//! validation rules from `docs/reference/configuration.md`.

mod address;
mod env;
#[cfg(feature = "serde")]
mod file_format;
pub(crate) mod reload;
mod text;
mod validate;

use std::net::SocketAddr;
use std::time::Duration;

/// The server's settings.
///
/// Every field is public and has a default, so a configuration can be built
/// with `..ServerConfig::default()`. `bind_addresses` has no useful default and
/// must be set; [`ServerConfig::validate`] reports a configuration without one.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct ServerConfig {
    /// The addresses to listen on.
    #[cfg_attr(feature = "serde", serde(with = "file_format::bind_addresses"))]
    pub bind_addresses: Vec<SocketAddr>,
    /// What to do when a listener cannot bind.
    pub bind_mode: BindMode,
    /// The listen backlog of each listener.
    pub backlog: u32,
    /// The most connections served at once.
    pub max_connections: usize,
    /// Per-connection timeouts.
    pub timeouts: TimeoutConfig,
    /// Socket options for listeners and connections.
    pub socket_options: SocketOptions,
    /// Graceful shutdown limits.
    pub shutdown: ShutdownConfig,
    /// Limits on file descriptors and memory.
    pub resource_limits: ResourceLimitsConfig,
    /// What the server records for telemetry.
    pub telemetry: TelemetryConfig,
}

impl Default for ServerConfig {
    fn default() -> ServerConfig {
        ServerConfig {
            bind_addresses: Vec::new(),
            bind_mode: BindMode::default(),
            backlog: 1024,
            max_connections: 10_000,
            timeouts: TimeoutConfig::default(),
            socket_options: SocketOptions::default(),
            shutdown: ShutdownConfig::default(),
            resource_limits: ResourceLimitsConfig::default(),
            telemetry: TelemetryConfig::default(),
        }
    }
}

/// What the server does when a listener cannot bind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[non_exhaustive]
pub enum BindMode {
    /// Fail at once.
    #[default]
    FailFast,
    /// Keep the listeners that bound, and report the failures.
    BestEffort,
}

/// Per-connection timeouts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct TimeoutConfig {
    /// From `accept()` until the MTA's first command arrives.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub connect: Duration,
    /// From the first byte of a frame until the frame is complete.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub read: Duration,
    /// While one reply is being written.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub write: Duration,
    /// While the connection waits for the MTA's next command.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub idle: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> TimeoutConfig {
        TimeoutConfig {
            connect: Duration::from_secs(10),
            read: Duration::from_secs(30),
            write: Duration::from_secs(30),
            idle: Duration::from_secs(7210),
        }
    }
}

/// Socket options for listeners and accepted connections.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct SocketOptions {
    /// `TCP_NODELAY` on accepted connections.
    pub tcp_nodelay: bool,
    /// TCP keepalive on accepted connections; `None` turns it off.
    #[cfg_attr(feature = "serde", serde(with = "file_format::keepalive"))]
    pub keepalive: Option<KeepaliveConfig>,
    /// `SO_RCVBUF`; `None` keeps the operating system's default.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub recv_buffer_size: Option<usize>,
    /// `SO_SNDBUF`; `None` keeps the operating system's default.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub send_buffer_size: Option<usize>,
    /// `SO_REUSEADDR` on listeners.
    pub reuse_addr: bool,
    /// `SO_REUSEPORT` on listeners.
    pub reuse_port: bool,
    /// `IPV6_V6ONLY` on IPv6 listeners.
    pub ipv6_only: bool,
}

impl Default for SocketOptions {
    fn default() -> SocketOptions {
        SocketOptions {
            tcp_nodelay: true,
            keepalive: Some(KeepaliveConfig::default()),
            recv_buffer_size: None,
            send_buffer_size: None,
            reuse_addr: true,
            reuse_port: false,
            ipv6_only: false,
        }
    }
}

/// TCP keepalive settings.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct KeepaliveConfig {
    /// Idle time before the first probe.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub time: Duration,
    /// Time between probes.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub interval: Duration,
    /// Probes sent before the connection is dropped.
    pub retries: u32,
}

impl Default for KeepaliveConfig {
    fn default() -> KeepaliveConfig {
        KeepaliveConfig {
            time: Duration::from_secs(60),
            interval: Duration::from_secs(10),
            retries: 6,
        }
    }
}

/// Graceful shutdown limits.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct ShutdownConfig {
    /// How long new connections are still accepted after a shutdown starts.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub grace_period: Duration,
    /// How long open sessions may take to finish.
    #[cfg_attr(feature = "serde", serde(with = "file_format::duration"))]
    pub drain_timeout: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> ShutdownConfig {
        ShutdownConfig {
            grace_period: Duration::from_secs(5),
            drain_timeout: Duration::from_secs(30),
        }
    }
}

/// Limits on file descriptors and memory.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct ResourceLimitsConfig {
    /// The most file descriptors the server uses; `None` for the system
    /// limit.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub max_fds: Option<usize>,
    /// The most memory for connection buffers; `None` for no limit.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub max_memory: Option<usize>,
    /// The buffer limit per connection, and the largest packet accepted.
    pub per_connection_buffer: usize,
}

impl Default for ResourceLimitsConfig {
    fn default() -> ResourceLimitsConfig {
        ResourceLimitsConfig {
            max_fds: None,
            max_memory: None,
            per_connection_buffer: 128 * 1024,
        }
    }
}

/// What the server records for telemetry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct TelemetryConfig {
    /// Which commands become trace events.
    pub trace_commands: TraceCommands,
}

/// Which milter commands the server records as trace events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[non_exhaustive]
pub enum TraceCommands {
    /// None.
    None,
    /// The envelope commands.
    #[default]
    Envelope,
    /// Every command.
    All,
}
