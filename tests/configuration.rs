//! Scenarios from `docs/testing/configuration.md`.

use std::net::SocketAddr;
use std::time::Duration;

use rust_milter::{AddressError, ConfigError, KeepaliveConfig, ServerConfig};

const KB: usize = 1024;
const MB: usize = 1024 * 1024;

/// A valid configuration: one bind address, every other field at its
/// default.
fn valid() -> ServerConfig {
    ServerConfig {
        bind_addresses: vec!["127.0.0.1:8891".parse().unwrap()],
        ..ServerConfig::default()
    }
}

/// One way to set a bounded field to a test value.
type Setter = fn(&mut ServerConfig);

/// For every bounded field: its path, configurations at its minimum and
/// maximum, and configurations just outside them where the type can hold
/// such a value.
struct Bounds {
    field: &'static str,
    in_range: Vec<Setter>,
    out_of_range: Vec<Setter>,
}

fn bounded_fields() -> Vec<Bounds> {
    vec![
        Bounds {
            field: "bind_addresses",
            in_range: vec![
                |c| c.bind_addresses = vec!["127.0.0.1:1".parse().unwrap()],
                |c| c.bind_addresses = vec!["127.0.0.1:65535".parse().unwrap()],
            ],
            out_of_range: vec![|c| c.bind_addresses = vec!["127.0.0.1:0".parse().unwrap()]],
        },
        Bounds {
            field: "backlog",
            in_range: vec![|c| c.backlog = 1, |c| c.backlog = 65_535],
            out_of_range: vec![|c| c.backlog = 0, |c| c.backlog = 65_536],
        },
        Bounds {
            field: "max_connections",
            in_range: vec![|c| c.max_connections = 1, |c| c.max_connections = 1_000_000],
            out_of_range: vec![|c| c.max_connections = 0, |c| c.max_connections = 1_000_001],
        },
        Bounds {
            field: "timeouts.connect",
            in_range: vec![|c| c.timeouts.connect = Duration::from_millis(1), |c| {
                c.timeouts.connect = Duration::from_secs(300)
            }],
            out_of_range: vec![|c| c.timeouts.connect = Duration::from_micros(999), |c| {
                c.timeouts.connect = Duration::from_millis(300_001)
            }],
        },
        Bounds {
            field: "timeouts.read",
            in_range: vec![|c| c.timeouts.read = Duration::from_millis(1), |c| {
                c.timeouts.read = Duration::from_secs(3_600)
            }],
            out_of_range: vec![|c| c.timeouts.read = Duration::from_micros(999), |c| {
                c.timeouts.read = Duration::from_millis(3_600_001)
            }],
        },
        Bounds {
            field: "timeouts.write",
            in_range: vec![|c| c.timeouts.write = Duration::from_millis(1), |c| {
                c.timeouts.write = Duration::from_secs(3_600)
            }],
            out_of_range: vec![|c| c.timeouts.write = Duration::from_micros(999), |c| {
                c.timeouts.write = Duration::from_millis(3_600_001)
            }],
        },
        Bounds {
            field: "timeouts.idle",
            in_range: vec![|c| c.timeouts.idle = Duration::from_secs(1), |c| {
                c.timeouts.idle = Duration::from_secs(86_400)
            }],
            out_of_range: vec![|c| c.timeouts.idle = Duration::from_millis(999), |c| {
                c.timeouts.idle = Duration::from_millis(86_400_001)
            }],
        },
        Bounds {
            field: "socket_options.recv_buffer_size",
            in_range: vec![|c| c.socket_options.recv_buffer_size = Some(KB), |c| {
                c.socket_options.recv_buffer_size = Some(64 * MB)
            }],
            out_of_range: vec![|c| c.socket_options.recv_buffer_size = Some(KB - 1), |c| {
                c.socket_options.recv_buffer_size = Some(64 * MB + 1)
            }],
        },
        Bounds {
            field: "socket_options.send_buffer_size",
            in_range: vec![|c| c.socket_options.send_buffer_size = Some(KB), |c| {
                c.socket_options.send_buffer_size = Some(64 * MB)
            }],
            out_of_range: vec![|c| c.socket_options.send_buffer_size = Some(KB - 1), |c| {
                c.socket_options.send_buffer_size = Some(64 * MB + 1)
            }],
        },
        Bounds {
            field: "socket_options.keepalive.time",
            in_range: vec![|c| keepalive(c).time = Duration::from_secs(1), |c| {
                keepalive(c).time = Duration::from_secs(86_400)
            }],
            out_of_range: vec![|c| keepalive(c).time = Duration::from_millis(999), |c| {
                keepalive(c).time = Duration::from_millis(86_400_001)
            }],
        },
        Bounds {
            field: "socket_options.keepalive.interval",
            in_range: vec![|c| keepalive(c).interval = Duration::from_secs(1), |c| {
                keepalive(c).interval = Duration::from_secs(3_600)
            }],
            out_of_range: vec![
                |c| keepalive(c).interval = Duration::from_millis(999),
                |c| keepalive(c).interval = Duration::from_millis(3_600_001),
            ],
        },
        Bounds {
            field: "socket_options.keepalive.retries",
            in_range: vec![|c| keepalive(c).retries = 1, |c| keepalive(c).retries = 30],
            out_of_range: vec![|c| keepalive(c).retries = 0, |c| keepalive(c).retries = 31],
        },
        Bounds {
            field: "shutdown.grace_period",
            // The minimum is 0 s, which a Duration cannot go below.
            in_range: vec![|c| c.shutdown.grace_period = Duration::ZERO, |c| {
                c.shutdown.grace_period = Duration::from_secs(60)
            }],
            out_of_range: vec![|c| c.shutdown.grace_period = Duration::from_millis(60_001)],
        },
        Bounds {
            field: "shutdown.drain_timeout",
            in_range: vec![|c| c.shutdown.drain_timeout = Duration::from_secs(1), |c| {
                c.shutdown.drain_timeout = Duration::from_secs(300)
            }],
            out_of_range: vec![
                |c| c.shutdown.drain_timeout = Duration::from_millis(999),
                |c| c.shutdown.drain_timeout = Duration::from_millis(300_001),
            ],
        },
        Bounds {
            field: "resource_limits.per_connection_buffer",
            in_range: vec![|c| c.resource_limits.per_connection_buffer = 65_540, |c| {
                c.resource_limits.per_connection_buffer = 16 * MB
            }],
            out_of_range: vec![|c| c.resource_limits.per_connection_buffer = 65_539, |c| {
                c.resource_limits.per_connection_buffer = 16 * MB + 1
            }],
        },
    ]
}

/// The keepalive settings, which the default configuration has.
fn keepalive(config: &mut ServerConfig) -> &mut KeepaliveConfig {
    config
        .socket_options
        .keepalive
        .as_mut()
        .expect("keepalive is on by default")
}

// Validation

#[test]
fn cfg_001_requires_a_bind_address() {
    assert_eq!(
        ServerConfig::default().validate(),
        Err(ConfigError::MissingBindAddress)
    );
}

#[test]
fn cfg_002_accepts_one_bind_address_and_the_defaults() {
    assert_eq!(valid().validate(), Ok(()));
}

#[test]
fn cfg_003_rejects_each_bounded_field_outside_its_range() {
    for bounds in bounded_fields() {
        for (case, set) in bounds.out_of_range.iter().enumerate() {
            let mut config = valid();
            set(&mut config);

            let result = config.validate();

            assert!(
                matches!(&result, Err(ConfigError::OutOfRange { field, .. }) if *field == bounds.field),
                "{} case {case}: got {result:?}",
                bounds.field
            );
        }
    }
}

#[test]
fn cfg_004_accepts_each_bounded_field_at_its_limits() {
    for bounds in bounded_fields() {
        for (case, set) in bounds.in_range.iter().enumerate() {
            let mut config = valid();
            set(&mut config);

            assert_eq!(config.validate(), Ok(()), "{} case {case}", bounds.field);
        }
    }
}

#[test]
fn cfg_005_rejects_a_duplicate_bind_address() {
    let address: SocketAddr = "127.0.0.1:8891".parse().unwrap();
    let mut config = valid();
    config.bind_addresses = vec![address, address];

    assert_eq!(
        config.validate(),
        Err(ConfigError::DuplicateBindAddress(address))
    );
}

#[test]
fn cfg_006_requires_room_for_one_full_default_size_packet() {
    let mut config = valid();

    config.resource_limits.per_connection_buffer = 65_539;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::OutOfRange {
            field: "resource_limits.per_connection_buffer",
            ..
        })
    ));

    config.resource_limits.per_connection_buffer = 65_540;
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn cfg_007_requires_max_fds_to_cover_connections_listeners_and_10() {
    let mut config = valid();
    config.bind_addresses = vec![
        "127.0.0.1:8891".parse().unwrap(),
        "[::1]:8891".parse().unwrap(),
    ];
    config.max_connections = 1_000;

    config.resource_limits.max_fds = Some(1_011);
    assert!(
        matches!(config.validate(), Err(ConfigError::Inconsistent { .. })),
        "1,011 must fail"
    );

    config.resource_limits.max_fds = Some(1_012);
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn cfg_008_requires_max_memory_to_hold_one_connection_buffer() {
    let mut config = valid();
    config.resource_limits.max_memory = Some(config.resource_limits.per_connection_buffer - 1);

    assert!(matches!(
        config.validate(),
        Err(ConfigError::Inconsistent { .. })
    ));
}

#[test]
fn cfg_009_rejects_reuse_port_where_the_platform_lacks_it() {
    if cfg!(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    )) {
        eprintln!("skipped: this platform supports SO_REUSEPORT");
        return;
    }
    let mut config = valid();
    config.socket_options.reuse_port = true;

    assert!(matches!(
        config.validate(),
        Err(ConfigError::Unsupported {
            field: "socket_options.reuse_port",
            ..
        })
    ));
}

// Address parsing

#[test]
fn cfg_020_parses_socket_addresses_and_the_libmilter_forms() {
    let cases = [
        ("127.0.0.1:8891", "127.0.0.1:8891"),
        ("[::1]:8891", "[::1]:8891"),
        ("inet:8891@127.0.0.1", "127.0.0.1:8891"),
        ("inet6:8891@::1", "[::1]:8891"),
    ];

    for (text, expected) in cases {
        let expected: SocketAddr = expected.parse().unwrap();
        assert_eq!(
            ServerConfig::parse_bind_address(text),
            Ok(expected),
            "{text}"
        );
    }
}

#[test]
fn cfg_021_rejects_unix_domain_socket_addresses() {
    for text in ["unix:/run/milter.sock", "local:/run/milter.sock"] {
        assert_eq!(
            ServerConfig::parse_bind_address(text),
            Err(AddressError::InvalidFormat),
            "{text}"
        );
    }
}

#[test]
fn cfg_022_rejects_ports_0_and_70000() {
    for text in [
        "127.0.0.1:0",
        "127.0.0.1:70000",
        "inet:0@127.0.0.1",
        "inet:70000@127.0.0.1",
    ] {
        assert_eq!(
            ServerConfig::parse_bind_address(text),
            Err(AddressError::InvalidPort),
            "{text}"
        );
    }
}

#[test]
fn cfg_023_rejects_a_host_that_is_not_an_address() {
    assert_eq!(
        ServerConfig::parse_bind_address("inet:8891@not a host"),
        Err(AddressError::InvalidHost)
    );
}

#[test]
fn cfg_024_rejects_a_host_of_the_other_family() {
    for text in ["inet:8891@::1", "inet6:8891@127.0.0.1"] {
        assert_eq!(
            ServerConfig::parse_bind_address(text),
            Err(AddressError::InvalidHost),
            "{text}"
        );
    }
}

#[test]
fn cfg_025_reads_the_libmilter_forms_without_a_host() {
    let cases = [("inet:8891", "0.0.0.0:8891"), ("inet6:8891", "[::]:8891")];

    for (text, expected) in cases {
        let expected: SocketAddr = expected.parse().unwrap();
        assert_eq!(
            ServerConfig::parse_bind_address(text),
            Ok(expected),
            "{text}"
        );
    }
}

#[test]
fn cfg_026_reads_an_ipv6_scope_id() {
    let parsed = ServerConfig::parse_bind_address("[fe80::1%2]:8891").unwrap();

    let SocketAddr::V6(v6) = parsed else {
        panic!("not IPv6: {parsed}");
    };
    assert_eq!(v6.scope_id(), 2);
    assert_eq!(v6.port(), 8891);
}

#[test]
fn cfg_027_names_the_form_for_postfix_text_and_reads_an_inet6_scope_id() {
    assert_eq!(
        ServerConfig::parse_bind_address("inet:127.0.0.1:8891"),
        Err(AddressError::InvalidFormat)
    );

    let parsed = ServerConfig::parse_bind_address("inet6:8891@fe80::1%2").unwrap();
    let SocketAddr::V6(v6) = parsed else {
        panic!("not IPv6: {parsed}");
    };
    assert_eq!((v6.port(), v6.scope_id()), (8891, 2));
}
