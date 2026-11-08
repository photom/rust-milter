//! The reload class of every setting, and the names of the settings that
//! differ between two configurations. See "Reload classes" in
//! `docs/reference/configuration.md`.

use super::{KeepaliveConfig, ServerConfig};

/// When a changed value takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReloadClass {
    /// As soon as the new generation is published.
    Immediate,
    /// For connections accepted after the reload.
    NewConnections,
    /// Only after a restart; a reload that changes it fails.
    Restart,
}

impl ReloadClass {
    /// The class's name in configuration.md.
    #[cfg(test)]
    fn name(self) -> &'static str {
        match self {
            ReloadClass::Immediate => "immediate",
            ReloadClass::NewConnections => "new_connections",
            ReloadClass::Restart => "restart",
        }
    }
}

/// One setting of [`ServerConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Setting {
    BindAddresses,
    BindMode,
    Backlog,
    MaxConnections,
    TimeoutsConnect,
    TimeoutsRead,
    TimeoutsWrite,
    TimeoutsIdle,
    TcpNodelay,
    Keepalive,
    RecvBufferSize,
    SendBufferSize,
    ReuseAddr,
    ReusePort,
    Ipv6Only,
    KeepaliveTime,
    KeepaliveInterval,
    KeepaliveRetries,
    GracePeriod,
    DrainTimeout,
    MaxFds,
    MaxMemory,
    PerConnectionBuffer,
    TraceCommands,
}

/// Every setting of [`ServerConfig`], in the order of configuration.md.
pub(crate) const SETTINGS: [Setting; 24] = [
    Setting::BindAddresses,
    Setting::BindMode,
    Setting::Backlog,
    Setting::MaxConnections,
    Setting::TimeoutsConnect,
    Setting::TimeoutsRead,
    Setting::TimeoutsWrite,
    Setting::TimeoutsIdle,
    Setting::TcpNodelay,
    Setting::Keepalive,
    Setting::RecvBufferSize,
    Setting::SendBufferSize,
    Setting::ReuseAddr,
    Setting::ReusePort,
    Setting::Ipv6Only,
    Setting::KeepaliveTime,
    Setting::KeepaliveInterval,
    Setting::KeepaliveRetries,
    Setting::GracePeriod,
    Setting::DrainTimeout,
    Setting::MaxFds,
    Setting::MaxMemory,
    Setting::PerConnectionBuffer,
    Setting::TraceCommands,
];

impl Setting {
    /// The setting's path in configuration.md.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Setting::BindAddresses => "bind_addresses",
            Setting::BindMode => "bind_mode",
            Setting::Backlog => "backlog",
            Setting::MaxConnections => "max_connections",
            Setting::TimeoutsConnect => "timeouts.connect",
            Setting::TimeoutsRead => "timeouts.read",
            Setting::TimeoutsWrite => "timeouts.write",
            Setting::TimeoutsIdle => "timeouts.idle",
            Setting::TcpNodelay => "socket_options.tcp_nodelay",
            Setting::Keepalive => "socket_options.keepalive",
            Setting::RecvBufferSize => "socket_options.recv_buffer_size",
            Setting::SendBufferSize => "socket_options.send_buffer_size",
            Setting::ReuseAddr => "socket_options.reuse_addr",
            Setting::ReusePort => "socket_options.reuse_port",
            Setting::Ipv6Only => "socket_options.ipv6_only",
            Setting::KeepaliveTime => "socket_options.keepalive.time",
            Setting::KeepaliveInterval => "socket_options.keepalive.interval",
            Setting::KeepaliveRetries => "socket_options.keepalive.retries",
            Setting::GracePeriod => "shutdown.grace_period",
            Setting::DrainTimeout => "shutdown.drain_timeout",
            Setting::MaxFds => "resource_limits.max_fds",
            Setting::MaxMemory => "resource_limits.max_memory",
            Setting::PerConnectionBuffer => "resource_limits.per_connection_buffer",
            Setting::TraceCommands => "telemetry.trace_commands",
        }
    }

    /// When a changed value takes effect.
    pub(crate) fn class(self) -> ReloadClass {
        match self {
            Setting::BindAddresses
            | Setting::BindMode
            | Setting::MaxConnections
            | Setting::GracePeriod
            | Setting::DrainTimeout
            | Setting::MaxFds
            | Setting::MaxMemory => ReloadClass::Immediate,
            Setting::TimeoutsConnect
            | Setting::TimeoutsRead
            | Setting::TimeoutsWrite
            | Setting::TimeoutsIdle
            | Setting::TcpNodelay
            | Setting::Keepalive
            | Setting::RecvBufferSize
            | Setting::SendBufferSize
            | Setting::KeepaliveTime
            | Setting::KeepaliveInterval
            | Setting::KeepaliveRetries
            | Setting::PerConnectionBuffer
            | Setting::TraceCommands => ReloadClass::NewConnections,
            Setting::Backlog | Setting::ReuseAddr | Setting::ReusePort | Setting::Ipv6Only => {
                ReloadClass::Restart
            }
        }
    }

    /// Whether `old` and `new` differ in this setting. Turning keepalive on
    /// or off counts as a change of `socket_options.keepalive` alone, so the
    /// keepalive fields differ only when both configurations turn it on.
    pub(crate) fn differs(self, old: &ServerConfig, new: &ServerConfig) -> bool {
        match self {
            Setting::BindAddresses => old.bind_addresses != new.bind_addresses,
            Setting::BindMode => old.bind_mode != new.bind_mode,
            Setting::Backlog => old.backlog != new.backlog,
            Setting::MaxConnections => old.max_connections != new.max_connections,
            Setting::TimeoutsConnect => old.timeouts.connect != new.timeouts.connect,
            Setting::TimeoutsRead => old.timeouts.read != new.timeouts.read,
            Setting::TimeoutsWrite => old.timeouts.write != new.timeouts.write,
            Setting::TimeoutsIdle => old.timeouts.idle != new.timeouts.idle,
            Setting::TcpNodelay => old.socket_options.tcp_nodelay != new.socket_options.tcp_nodelay,
            Setting::Keepalive => {
                old.socket_options.keepalive.is_some() != new.socket_options.keepalive.is_some()
            }
            Setting::RecvBufferSize => {
                old.socket_options.recv_buffer_size != new.socket_options.recv_buffer_size
            }
            Setting::SendBufferSize => {
                old.socket_options.send_buffer_size != new.socket_options.send_buffer_size
            }
            Setting::ReuseAddr => old.socket_options.reuse_addr != new.socket_options.reuse_addr,
            Setting::ReusePort => old.socket_options.reuse_port != new.socket_options.reuse_port,
            Setting::Ipv6Only => old.socket_options.ipv6_only != new.socket_options.ipv6_only,
            Setting::KeepaliveTime => match both_keepalive(old, new) {
                Some((old, new)) => old.time != new.time,
                None => false,
            },
            Setting::KeepaliveInterval => match both_keepalive(old, new) {
                Some((old, new)) => old.interval != new.interval,
                None => false,
            },
            Setting::KeepaliveRetries => match both_keepalive(old, new) {
                Some((old, new)) => old.retries != new.retries,
                None => false,
            },
            Setting::GracePeriod => old.shutdown.grace_period != new.shutdown.grace_period,
            Setting::DrainTimeout => old.shutdown.drain_timeout != new.shutdown.drain_timeout,
            Setting::MaxFds => old.resource_limits.max_fds != new.resource_limits.max_fds,
            Setting::MaxMemory => old.resource_limits.max_memory != new.resource_limits.max_memory,
            Setting::PerConnectionBuffer => {
                old.resource_limits.per_connection_buffer
                    != new.resource_limits.per_connection_buffer
            }
            Setting::TraceCommands => old.telemetry.trace_commands != new.telemetry.trace_commands,
        }
    }
}

/// The keepalive settings of both configurations, when both turn it on.
fn both_keepalive<'a>(
    old: &'a ServerConfig,
    new: &'a ServerConfig,
) -> Option<(&'a KeepaliveConfig, &'a KeepaliveConfig)> {
    match (&old.socket_options.keepalive, &new.socket_options.keepalive) {
        (Some(old), Some(new)) => Some((old, new)),
        _ => None,
    }
}

/// The names of the settings in which `new` differs from `old`.
pub(crate) fn changed(old: &ServerConfig, new: &ServerConfig) -> Vec<&'static str> {
    let mut names = Vec::new();
    for setting in SETTINGS {
        if setting.differs(old, new) {
            names.push(setting.name());
        }
    }
    names
}

/// The names of the settings that require a restart and in which `new`
/// differs from `old`.
pub(crate) fn requires_restart(old: &ServerConfig, new: &ServerConfig) -> Vec<&'static str> {
    let mut names = Vec::new();
    for setting in SETTINGS {
        if setting.class() == ReloadClass::Restart && setting.differs(old, new) {
            names.push(setting.name());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The setting names and reload classes in the "ServerConfig reference"
    /// tables of configuration.md, in their order.
    fn documented() -> Vec<(String, String)> {
        let text = include_str!("../../docs/reference/configuration.md");
        let start = text.find("## ServerConfig reference").unwrap();
        let end = text.find("## Builder-only settings").unwrap();
        let mut prefix = "";
        let mut rows = Vec::new();
        for line in text[start..end].lines() {
            if let Some(section) = line.strip_prefix("### ") {
                prefix = match section {
                    "Top level" => "",
                    "TimeoutConfig" => "timeouts.",
                    "SocketOptions" => "socket_options.",
                    "KeepaliveConfig" => "socket_options.keepalive.",
                    "ShutdownConfig" => "shutdown.",
                    "ResourceLimitsConfig" => "resource_limits.",
                    "TelemetryConfig" => "telemetry.",
                    other => panic!("unknown section {other:?}"),
                };
                continue;
            }
            let mut cells: Vec<&str> = Vec::new();
            for cell in line.split('|') {
                cells.push(cell.trim());
            }
            // | field | type | default | valid values | reload class |
            if cells.len() != 7 || !cells[1].starts_with('`') {
                continue;
            }
            let class = cells[5].trim_matches('`');
            if class.is_empty() {
                continue;
            }
            let field = cells[1].trim_matches('`');
            rows.push((format!("{prefix}{field}"), class.to_owned()));
        }
        rows
    }

    #[test]
    fn cfg_060_every_setting_has_its_documented_reload_class() {
        let mut code = Vec::new();
        for setting in SETTINGS {
            code.push((setting.name().to_owned(), setting.class().name().to_owned()));
        }

        assert_eq!(code, documented());
    }

    #[test]
    fn rld_006_names_the_changed_settings_in_document_order() {
        let first = ServerConfig::default();
        let mut second = first.clone();
        second.max_connections = 500;
        second.timeouts.idle = std::time::Duration::from_secs(60);
        second.socket_options.keepalive.as_mut().unwrap().time =
            std::time::Duration::from_secs(120);
        let mut third = first.clone();
        third.socket_options.keepalive = None;

        assert_eq!(
            changed(&first, &second),
            [
                "max_connections",
                "timeouts.idle",
                "socket_options.keepalive.time"
            ]
        );
        assert_eq!(changed(&first, &third), ["socket_options.keepalive"]);
    }
}
