//! File format scenarios (CFG-040 to CFG-051) from
//! `docs/testing/configuration.md`. They need the `serde` feature.

#![cfg(feature = "serde")]

use std::time::Duration;

use rust_milter::{BindMode, ConfigError, KeepaliveConfig, ServerConfig, TraceCommands};

/// A TOML document with one bind address and `rest` after it.
fn document(rest: &str) -> String {
    format!("bind_addresses = [\"127.0.0.1:8891\"]\n{rest}")
}

fn from_toml(text: &str) -> Result<ServerConfig, toml::de::Error> {
    toml::from_str(text)
}

#[test]
fn cfg_040_fills_every_missing_key_with_its_default() {
    let config = from_toml(&document("")).unwrap();

    let expected = ServerConfig {
        bind_addresses: vec!["127.0.0.1:8891".parse().unwrap()],
        ..ServerConfig::default()
    };
    assert_eq!(config, expected);
}

#[test]
fn cfg_041_rejects_an_unknown_key_and_names_it() {
    let error = from_toml(&document("max_conections = 500\n")).unwrap_err();

    assert!(
        error.to_string().contains("max_conections"),
        "the error does not name the key: {error}"
    );
}

#[test]
fn cfg_042_reads_durations_with_a_unit() {
    let cases = [
        ("500ms", Duration::from_millis(500)),
        ("30s", Duration::from_secs(30)),
        ("5m", Duration::from_secs(300)),
        ("1h", Duration::from_secs(3_600)),
    ];

    for (text, expected) in cases {
        let config = from_toml(&document(&format!("[timeouts]\nread = \"{text}\"\n"))).unwrap();
        assert_eq!(config.timeouts.read, expected, "{text}");
    }
}

#[test]
fn cfg_043_rejects_durations_without_a_known_unit() {
    for value in ["30", "\"30\"", "\"30sec\""] {
        let result = from_toml(&document(&format!("[timeouts]\nread = {value}\n")));
        assert!(result.is_err(), "{value} was accepted");
    }
}

#[test]
fn cfg_044_reads_enums_in_lowercase_with_underscores() {
    let config = from_toml(&document("bind_mode = \"best_effort\"\n")).unwrap();
    assert_eq!(config.bind_mode, BindMode::BestEffort);

    assert!(from_toml(&document("bind_mode = \"BestEffort\"\n")).is_err());
}

#[test]
fn cfg_045_reads_trace_commands_in_the_telemetry_table() {
    let config = from_toml(&document("[telemetry]\ntrace_commands = \"all\"\n")).unwrap();

    assert_eq!(config.telemetry.trace_commands, TraceCommands::All);
}

#[test]
fn cfg_046_reads_keepalive_as_false_or_as_a_table() {
    let off = from_toml(&document("[socket_options]\nkeepalive = false\n")).unwrap();
    assert_eq!(off.socket_options.keepalive, None);

    let table = from_toml(&document("[socket_options.keepalive]\ntime = \"30s\"\n")).unwrap();
    assert_eq!(
        table.socket_options.keepalive,
        Some(KeepaliveConfig {
            time: Duration::from_secs(30),
            ..KeepaliveConfig::default()
        })
    );
}

#[test]
fn cfg_047_reads_nested_tables_into_the_matching_parts() {
    let text = document(
        "[timeouts]\nidle = \"120s\"\n\
         [shutdown]\ndrain_timeout = \"10s\"\n\
         [resource_limits]\nper_connection_buffer = 262144\n",
    );

    let config = from_toml(&text).unwrap();

    assert_eq!(config.timeouts.idle, Duration::from_secs(120));
    assert_eq!(config.shutdown.drain_timeout, Duration::from_secs(10));
    assert_eq!(config.resource_limits.per_connection_buffer, 262_144);
}

#[test]
fn cfg_048_writes_every_field_and_reads_back_the_same_configuration() {
    let mut config = ServerConfig {
        bind_addresses: vec![
            "127.0.0.1:8891".parse().unwrap(),
            "[::1]:8891".parse().unwrap(),
        ],
        bind_mode: BindMode::BestEffort,
        backlog: 2_048,
        max_connections: 500,
        ..ServerConfig::default()
    };
    config.timeouts.read = Duration::from_millis(1_500);
    config.timeouts.connect = Duration::from_secs(300);
    config.socket_options.keepalive = None;
    config.socket_options.recv_buffer_size = Some(65_536);
    config.socket_options.send_buffer_size = Some(65_536);
    config.socket_options.reuse_port = true;
    config.shutdown.grace_period = Duration::ZERO;
    config.resource_limits.max_fds = Some(1_000);
    config.resource_limits.max_memory = Some(1 << 30);
    config.telemetry.trace_commands = TraceCommands::None;

    let text = toml::to_string(&config).unwrap();
    let read_back = from_toml(&text).unwrap();

    assert_eq!(read_back, config, "TOML:\n{text}");
    assert!(text.contains("keepalive = false"), "TOML:\n{text}");
    for key in [
        "bind_addresses",
        "bind_mode",
        "backlog",
        "max_connections",
        "connect",
        "read",
        "write",
        "idle",
        "tcp_nodelay",
        "keepalive",
        "recv_buffer_size",
        "send_buffer_size",
        "reuse_addr",
        "reuse_port",
        "ipv6_only",
        "grace_period",
        "drain_timeout",
        "max_fds",
        "max_memory",
        "per_connection_buffer",
        "trace_commands",
    ] {
        let mut found = false;
        for line in text.lines() {
            if line.starts_with(&format!("{key} =")) {
                found = true;
            }
        }
        assert!(found, "{key} is missing from:\n{text}");
    }
}

#[test]
fn cfg_049_reads_an_invalid_value_and_leaves_it_to_validation() {
    let config = from_toml(&document("backlog = 0\n")).unwrap();

    assert!(matches!(
        config.validate(),
        Err(ConfigError::OutOfRange {
            field: "backlog",
            ..
        })
    ));
}

#[test]
fn cfg_048_writes_the_defaults_with_keepalive_on_and_reads_them_back() {
    let config = from_toml(&document("")).unwrap();

    let text = toml::to_string(&config).unwrap();

    assert_eq!(from_toml(&text).unwrap(), config, "TOML:\n{text}");
    assert!(text.contains("[socket_options.keepalive]"), "TOML:\n{text}");
}

#[test]
fn cfg_050_rejects_keepalive_true() {
    assert!(from_toml(&document("[socket_options]\nkeepalive = true\n")).is_err());
}

#[test]
fn cfg_051_cannot_write_a_duration_finer_than_a_millisecond() {
    let mut config = from_toml(&document("")).unwrap();
    config.timeouts.read = Duration::from_micros(1_500);

    assert!(toml::to_string(&config).is_err());
}

#[test]
fn cfg_052_round_trips_an_ipv6_scope_id() {
    let address: std::net::SocketAddr =
        std::net::SocketAddrV6::new("fe80::1".parse().unwrap(), 8891, 0, 2).into();
    let config = ServerConfig {
        bind_addresses: vec![address],
        ..ServerConfig::default()
    };

    let text = toml::to_string(&config).unwrap();

    assert_eq!(from_toml(&text).unwrap(), config, "TOML:\n{text}");
}
