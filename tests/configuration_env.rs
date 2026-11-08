//! Environment variable scenarios (CFG-080 to CFG-093) from
//! `docs/testing/configuration.md`. They use `from_vars` and `apply_vars`, so
//! they never change the process environment.

use std::collections::BTreeSet;
use std::time::Duration;

use rust_milter::{BindMode, ConfigError, ServerConfig, TraceCommands};

const PREFIX: &str = "RUST_MILTER_";

fn from_vars(vars: &[(&str, &str)]) -> Result<ServerConfig, ConfigError> {
    ServerConfig::from_vars(PREFIX, vars.iter().copied())
}

#[test]
fn cfg_080_reads_a_list_of_bind_addresses() {
    let config = from_vars(&[("RUST_MILTER_BIND_ADDRESSES", "0.0.0.0:8891, [::]:8891")]).unwrap();

    let expected = ServerConfig {
        bind_addresses: vec![
            "0.0.0.0:8891".parse().unwrap(),
            "[::]:8891".parse().unwrap(),
        ],
        ..ServerConfig::default()
    };
    assert_eq!(config, expected);
}

#[test]
fn cfg_081_overrides_only_the_variables_that_are_set() {
    let mut config = ServerConfig {
        max_connections: 1_000,
        backlog: 2_048,
        ..ServerConfig::default()
    };

    config
        .apply_vars(PREFIX, [("RUST_MILTER_MAX_CONNECTIONS", "500")])
        .unwrap();

    assert_eq!(config.max_connections, 500);
    assert_eq!(config.backlog, 2_048);
}

#[test]
fn cfg_082_reads_nested_settings_with_double_underscores() {
    let config = from_vars(&[
        ("RUST_MILTER_TIMEOUTS__IDLE", "120s"),
        ("RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE__TIME", "30s"),
    ])
    .unwrap();

    assert_eq!(config.timeouts.idle, Duration::from_secs(120));
    assert_eq!(
        config.socket_options.keepalive.unwrap().time,
        Duration::from_secs(30)
    );
}

#[test]
fn cfg_083_turns_keepalive_off_with_false() {
    let config = from_vars(&[("RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE", "false")]).unwrap();

    assert_eq!(config.socket_options.keepalive, None);
}

#[test]
fn cfg_084_rejects_a_misspelled_variable() {
    assert_eq!(
        from_vars(&[("RUST_MILTER_MAX_CONECTIONS", "500")]),
        Err(ConfigError::UnknownEnvVariable(
            "RUST_MILTER_MAX_CONECTIONS".to_owned()
        ))
    );
}

#[test]
fn cfg_085_names_the_variable_but_not_the_value_of_a_bad_value() {
    let error = from_vars(&[("RUST_MILTER_BACKLOG", "many")]).unwrap_err();

    assert!(
        matches!(&error, ConfigError::InvalidEnvValue { variable, .. } if variable == "RUST_MILTER_BACKLOG"),
        "got {error:?}"
    );
    assert!(!error.to_string().contains("many"), "{error}");
    assert!(!format!("{error:?}").contains("many"), "{error:?}");
}

#[test]
fn cfg_086_reads_only_the_given_prefix() {
    let config = ServerConfig::from_vars(
        "MYFILTER_",
        [
            ("MYFILTER_MAX_CONNECTIONS", "200"),
            ("RUST_MILTER_MAX_CONNECTIONS", "300"),
        ],
    )
    .unwrap();

    assert_eq!(config.max_connections, 200);
}

#[test]
fn cfg_087_ignores_variables_without_the_prefix() {
    let config = from_vars(&[
        ("PATH", "/usr/bin"),
        (
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "http://collector.void.invalid:4317",
        ),
    ])
    .unwrap();

    assert_eq!(config, ServerConfig::default());
}

#[test]
fn cfg_088_reads_enums_in_the_file_format() {
    let config = from_vars(&[("RUST_MILTER_BIND_MODE", "best_effort")]).unwrap();
    assert_eq!(config.bind_mode, BindMode::BestEffort);

    assert!(matches!(
        from_vars(&[("RUST_MILTER_BIND_MODE", "BestEffort")]),
        Err(ConfigError::InvalidEnvValue { .. })
    ));
}

#[test]
fn cfg_089_checks_formats_only_and_leaves_ranges_to_validation() {
    let mut config = from_vars(&[("RUST_MILTER_BACKLOG", "0")]).unwrap();
    config.bind_addresses = vec!["127.0.0.1:8891".parse().unwrap()];

    assert!(matches!(
        config.validate(),
        Err(ConfigError::OutOfRange {
            field: "backlog",
            ..
        })
    ));
}

/// Every setting: its path, and a value for its variable. [`expect`] makes
/// the change that value makes to the defaults.
const SETTINGS: [(&str, &str); 24] = [
    ("bind_addresses", "127.0.0.1:9999"),
    ("bind_mode", "best_effort"),
    ("backlog", "2048"),
    ("max_connections", "500"),
    ("timeouts.connect", "42s"),
    ("timeouts.read", "42s"),
    ("timeouts.write", "42s"),
    ("timeouts.idle", "42s"),
    ("socket_options.tcp_nodelay", "false"),
    ("socket_options.keepalive", "false"),
    ("socket_options.recv_buffer_size", "4096"),
    ("socket_options.send_buffer_size", "4096"),
    ("socket_options.reuse_addr", "false"),
    ("socket_options.reuse_port", "true"),
    ("socket_options.ipv6_only", "true"),
    ("socket_options.keepalive.time", "42s"),
    ("socket_options.keepalive.interval", "42s"),
    ("socket_options.keepalive.retries", "3"),
    ("shutdown.grace_period", "42s"),
    ("shutdown.drain_timeout", "42s"),
    ("resource_limits.max_fds", "20000"),
    ("resource_limits.max_memory", "1048576"),
    ("resource_limits.per_connection_buffer", "262144"),
    ("telemetry.trace_commands", "all"),
];

/// Changes `config` as the value of `SETTINGS` for `path` does.
fn expect(path: &str, config: &mut ServerConfig) {
    const D: Duration = Duration::from_secs(42);
    match path {
        "bind_addresses" => config.bind_addresses = vec!["127.0.0.1:9999".parse().unwrap()],
        "bind_mode" => config.bind_mode = BindMode::BestEffort,
        "backlog" => config.backlog = 2_048,
        "max_connections" => config.max_connections = 500,
        "timeouts.connect" => config.timeouts.connect = D,
        "timeouts.read" => config.timeouts.read = D,
        "timeouts.write" => config.timeouts.write = D,
        "timeouts.idle" => config.timeouts.idle = D,
        "socket_options.tcp_nodelay" => config.socket_options.tcp_nodelay = false,
        "socket_options.keepalive" => config.socket_options.keepalive = None,
        "socket_options.recv_buffer_size" => config.socket_options.recv_buffer_size = Some(4_096),
        "socket_options.send_buffer_size" => config.socket_options.send_buffer_size = Some(4_096),
        "socket_options.reuse_addr" => config.socket_options.reuse_addr = false,
        "socket_options.reuse_port" => config.socket_options.reuse_port = true,
        "socket_options.ipv6_only" => config.socket_options.ipv6_only = true,
        "socket_options.keepalive.time" => keepalive(config).time = D,
        "socket_options.keepalive.interval" => keepalive(config).interval = D,
        "socket_options.keepalive.retries" => keepalive(config).retries = 3,
        "shutdown.grace_period" => config.shutdown.grace_period = D,
        "shutdown.drain_timeout" => config.shutdown.drain_timeout = D,
        "resource_limits.max_fds" => config.resource_limits.max_fds = Some(20_000),
        "resource_limits.max_memory" => config.resource_limits.max_memory = Some(1_048_576),
        "resource_limits.per_connection_buffer" => {
            config.resource_limits.per_connection_buffer = 262_144
        }
        "telemetry.trace_commands" => config.telemetry.trace_commands = TraceCommands::All,
        other => panic!("no expected change for {other}"),
    }
}

fn keepalive(config: &mut ServerConfig) -> &mut rust_milter::KeepaliveConfig {
    config.socket_options.keepalive.as_mut().unwrap()
}

/// The setting paths in the reference tables of `configuration.md`: every
/// row with a reload class, under the part its section describes.
fn documented_settings() -> BTreeSet<String> {
    let doc = include_str!("../docs/reference/configuration.md");
    let rest = doc
        .split("## ServerConfig reference")
        .nth(1)
        .expect("configuration.md has the reference tables");
    let reference = rest
        .split("## Builder-only settings")
        .next()
        .expect("configuration.md has the reference tables");

    let mut settings = BTreeSet::new();
    let mut part = "";
    for line in reference.lines() {
        if let Some(heading) = line.strip_prefix("### ") {
            part = match heading {
                "Top level" => "",
                "TimeoutConfig" => "timeouts.",
                "SocketOptions" => "socket_options.",
                "KeepaliveConfig" => "socket_options.keepalive.",
                "ShutdownConfig" => "shutdown.",
                "ResourceLimitsConfig" => "resource_limits.",
                "TelemetryConfig" => "telemetry.",
                other => panic!("unexpected section {other:?} in configuration.md"),
            };
            continue;
        }
        let mut cells = Vec::new();
        for cell in line.split('|') {
            cells.push(cell.trim());
        }
        // | `field` | `Type` | default | valid values | reload class |
        if cells.len() == 7 && cells[1].starts_with('`') && !cells[5].is_empty() {
            settings.insert(format!("{part}{}", cells[1].trim_matches('`')));
        }
    }
    settings
}

#[test]
fn cfg_090_every_documented_setting_has_a_working_variable() {
    let mut tested = BTreeSet::new();
    for (path, _) in SETTINGS {
        tested.insert(path.to_owned());
    }
    assert_eq!(
        tested,
        documented_settings(),
        "the test's settings differ from the tables in configuration.md"
    );

    for (path, value) in SETTINGS {
        let variable = format!("{PREFIX}{}", path.replace('.', "__").to_uppercase());
        let mut expected = ServerConfig::default();
        expect(path, &mut expected);

        let config = from_vars(&[(&variable, value)]);

        assert_eq!(config, Ok(expected), "{variable}={value}");
    }
}

#[test]
fn cfg_091_rejects_keepalive_off_together_with_keepalive_settings() {
    let result = from_vars(&[
        ("RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE", "false"),
        ("RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE__TIME", "30s"),
    ]);

    assert!(
        matches!(&result, Err(ConfigError::InvalidEnvValue { variable, .. }) if variable == "RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE"),
        "got {result:?}"
    );
}

#[test]
fn cfg_092_leaves_the_configuration_unchanged_when_a_variable_fails() {
    let mut config = ServerConfig {
        max_connections: 1_000,
        ..ServerConfig::default()
    };

    let result = config.apply_vars(
        PREFIX,
        [
            ("RUST_MILTER_MAX_CONNECTIONS", "500"),
            ("RUST_MILTER_BACKLOG", "many"),
        ],
    );

    assert!(matches!(result, Err(ConfigError::InvalidEnvValue { .. })));
    assert_eq!(config.max_connections, 1_000);
}

#[test]
fn cfg_093_requires_variable_names_in_upper_case() {
    assert_eq!(
        from_vars(&[("RUST_MILTER_max_connections", "500")]),
        Err(ConfigError::UnknownEnvVariable(
            "RUST_MILTER_max_connections".to_owned()
        ))
    );
}
