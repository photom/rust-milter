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

/// One setting: its path, its class, and whether two configurations differ
/// in it.
pub(crate) struct Setting {
    pub(crate) name: &'static str,
    pub(crate) class: ReloadClass,
    differs: fn(&ServerConfig, &ServerConfig) -> bool,
}

impl Setting {
    pub(crate) fn differs(&self, old: &ServerConfig, new: &ServerConfig) -> bool {
        (self.differs)(old, new)
    }
}

/// Whether two configurations differ in the value `$get` reads.
macro_rules! setting {
    ($name:literal, $class:ident, |$config:ident| $get:expr) => {
        Setting {
            name: $name,
            class: ReloadClass::$class,
            differs: |old, new| {
                let $config = old;
                let old = $get;
                let $config = new;
                old != $get
            },
        }
    };
}

/// Whether two configurations that both turn keepalive on differ in the
/// value `$get` reads. Turning keepalive on or off counts as a change of
/// `socket_options.keepalive` alone.
macro_rules! keepalive {
    ($name:literal, |$keepalive:ident| $get:expr) => {
        Setting {
            name: $name,
            class: ReloadClass::NewConnections,
            differs: |old, new| match (&old.socket_options.keepalive, &new.socket_options.keepalive)
            {
                (Some(old), Some(new)) => {
                    let $keepalive: &KeepaliveConfig = old;
                    let old = $get;
                    let $keepalive: &KeepaliveConfig = new;
                    old != $get
                }
                _ => false,
            },
        }
    };
}

/// Every setting of [`ServerConfig`], in the order of configuration.md.
pub(crate) static SETTINGS: &[Setting] = &[
    setting!("bind_addresses", Immediate, |c| &c.bind_addresses),
    setting!("bind_mode", Immediate, |c| c.bind_mode),
    setting!("backlog", Restart, |c| c.backlog),
    setting!("max_connections", Immediate, |c| c.max_connections),
    setting!("timeouts.connect", NewConnections, |c| c.timeouts.connect),
    setting!("timeouts.read", NewConnections, |c| c.timeouts.read),
    setting!("timeouts.write", NewConnections, |c| c.timeouts.write),
    setting!("timeouts.idle", NewConnections, |c| c.timeouts.idle),
    setting!("socket_options.tcp_nodelay", NewConnections, |c| {
        c.socket_options.tcp_nodelay
    }),
    setting!("socket_options.keepalive", NewConnections, |c| {
        c.socket_options.keepalive.is_some()
    }),
    setting!("socket_options.recv_buffer_size", NewConnections, |c| {
        c.socket_options.recv_buffer_size
    }),
    setting!("socket_options.send_buffer_size", NewConnections, |c| {
        c.socket_options.send_buffer_size
    }),
    setting!("socket_options.reuse_addr", Restart, |c| {
        c.socket_options.reuse_addr
    }),
    setting!("socket_options.reuse_port", Restart, |c| {
        c.socket_options.reuse_port
    }),
    setting!("socket_options.ipv6_only", Restart, |c| {
        c.socket_options.ipv6_only
    }),
    keepalive!("socket_options.keepalive.time", |k| k.time),
    keepalive!("socket_options.keepalive.interval", |k| k.interval),
    keepalive!("socket_options.keepalive.retries", |k| k.retries),
    setting!("shutdown.grace_period", Immediate, |c| {
        c.shutdown.grace_period
    }),
    setting!("shutdown.drain_timeout", Immediate, |c| {
        c.shutdown.drain_timeout
    }),
    setting!("resource_limits.max_fds", Immediate, |c| {
        c.resource_limits.max_fds
    }),
    setting!("resource_limits.max_memory", Immediate, |c| {
        c.resource_limits.max_memory
    }),
    setting!(
        "resource_limits.per_connection_buffer",
        NewConnections,
        |c| { c.resource_limits.per_connection_buffer }
    ),
    setting!("telemetry.trace_commands", NewConnections, |c| {
        c.telemetry.trace_commands
    }),
];

/// The names of the settings in which `new` differs from `old`.
pub(crate) fn changed(old: &ServerConfig, new: &ServerConfig) -> Vec<&'static str> {
    SETTINGS
        .iter()
        .filter(|setting| setting.differs(old, new))
        .map(|setting| setting.name)
        .collect()
}

/// The names of the settings that require a restart and in which `new`
/// differs from `old`.
pub(crate) fn requires_restart(old: &ServerConfig, new: &ServerConfig) -> Vec<&'static str> {
    SETTINGS
        .iter()
        .filter(|setting| setting.class == ReloadClass::Restart && setting.differs(old, new))
        .map(|setting| setting.name)
        .collect()
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
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
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
        let code: Vec<(String, String)> = SETTINGS
            .iter()
            .map(|setting| (setting.name.to_owned(), setting.class.name().to_owned()))
            .collect();

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
