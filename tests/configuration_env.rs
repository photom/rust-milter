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

/// One setting: its path, a value for its variable, and the change that value
/// makes to the defaults.
struct Setting {
    path: &'static str,
    value: &'static str,
    expect: fn(&mut ServerConfig),
}

fn every_setting() -> Vec<Setting> {
    const D: Duration = Duration::from_secs(42);
    fn keepalive(c: &mut ServerConfig) -> &mut rust_milter::KeepaliveConfig {
        c.socket_options.keepalive.as_mut().unwrap()
    }
    vec![
        Setting {
            path: "bind_addresses",
            value: "127.0.0.1:9999",
            expect: |c| c.bind_addresses = vec!["127.0.0.1:9999".parse().unwrap()],
        },
        Setting {
            path: "bind_mode",
            value: "best_effort",
            expect: |c| c.bind_mode = BindMode::BestEffort,
        },
        Setting {
            path: "backlog",
            value: "2048",
            expect: |c| c.backlog = 2_048,
        },
        Setting {
            path: "max_connections",
            value: "500",
            expect: |c| c.max_connections = 500,
        },
        Setting {
            path: "timeouts.connect",
            value: "42s",
            expect: |c| c.timeouts.connect = D,
        },
        Setting {
            path: "timeouts.read",
            value: "42s",
            expect: |c| c.timeouts.read = D,
        },
        Setting {
            path: "timeouts.write",
            value: "42s",
            expect: |c| c.timeouts.write = D,
        },
        Setting {
            path: "timeouts.idle",
            value: "42s",
            expect: |c| c.timeouts.idle = D,
        },
        Setting {
            path: "socket_options.tcp_nodelay",
            value: "false",
            expect: |c| c.socket_options.tcp_nodelay = false,
        },
        Setting {
            path: "socket_options.keepalive",
            value: "false",
            expect: |c| c.socket_options.keepalive = None,
        },
        Setting {
            path: "socket_options.recv_buffer_size",
            value: "4096",
            expect: |c| c.socket_options.recv_buffer_size = Some(4_096),
        },
        Setting {
            path: "socket_options.send_buffer_size",
            value: "4096",
            expect: |c| c.socket_options.send_buffer_size = Some(4_096),
        },
        Setting {
            path: "socket_options.reuse_addr",
            value: "false",
            expect: |c| c.socket_options.reuse_addr = false,
        },
        Setting {
            path: "socket_options.reuse_port",
            value: "true",
            expect: |c| c.socket_options.reuse_port = true,
        },
        Setting {
            path: "socket_options.ipv6_only",
            value: "true",
            expect: |c| c.socket_options.ipv6_only = true,
        },
        Setting {
            path: "socket_options.keepalive.time",
            value: "42s",
            expect: |c| keepalive(c).time = D,
        },
        Setting {
            path: "socket_options.keepalive.interval",
            value: "42s",
            expect: |c| keepalive(c).interval = D,
        },
        Setting {
            path: "socket_options.keepalive.retries",
            value: "3",
            expect: |c| keepalive(c).retries = 3,
        },
        Setting {
            path: "shutdown.grace_period",
            value: "42s",
            expect: |c| c.shutdown.grace_period = D,
        },
        Setting {
            path: "shutdown.drain_timeout",
            value: "42s",
            expect: |c| c.shutdown.drain_timeout = D,
        },
        Setting {
            path: "resource_limits.max_fds",
            value: "20000",
            expect: |c| c.resource_limits.max_fds = Some(20_000),
        },
        Setting {
            path: "resource_limits.max_memory",
            value: "1048576",
            expect: |c| c.resource_limits.max_memory = Some(1_048_576),
        },
        Setting {
            path: "resource_limits.per_connection_buffer",
            value: "262144",
            expect: |c| c.resource_limits.per_connection_buffer = 262_144,
        },
        Setting {
            path: "telemetry.trace_commands",
            value: "all",
            expect: |c| c.telemetry.trace_commands = TraceCommands::All,
        },
    ]
}

/// The setting paths in the reference tables of `configuration.md`: every
/// row with a reload class, under the part its section describes.
fn documented_settings() -> BTreeSet<String> {
    let doc = include_str!("../docs/reference/configuration.md");
    let reference = doc
        .split("## ServerConfig reference")
        .nth(1)
        .and_then(|rest| rest.split("## Builder-only settings").next())
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
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // | `field` | `Type` | default | valid values | reload class |
        if cells.len() == 7 && cells[1].starts_with('`') && !cells[5].is_empty() {
            settings.insert(format!("{part}{}", cells[1].trim_matches('`')));
        }
    }
    settings
}

#[test]
fn cfg_090_every_documented_setting_has_a_working_variable() {
    let tested: BTreeSet<String> = every_setting().iter().map(|s| s.path.to_owned()).collect();
    assert_eq!(
        tested,
        documented_settings(),
        "the test's settings differ from the tables in configuration.md"
    );

    for setting in every_setting() {
        let variable = format!("{PREFIX}{}", setting.path.replace('.', "__").to_uppercase());
        let mut expected = ServerConfig::default();
        (setting.expect)(&mut expected);

        let config = from_vars(&[(&variable, setting.value)]);

        assert_eq!(config, Ok(expected), "{variable}={}", setting.value);
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
