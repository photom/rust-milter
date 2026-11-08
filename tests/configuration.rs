//! Scenarios from `docs/testing/configuration.md`.

use std::net::SocketAddr;
use std::time::Duration;

use rust_milter::{AddressError, ConfigError, KeepaliveConfig, ServerConfig};

const KB: u64 = 1024;
const MB: u64 = 1024 * 1024;
/// A millisecond and a second, in the microseconds of [`BOUNDS`].
const MS: u64 = 1_000;
const S: u64 = 1_000_000;

/// A valid configuration: one bind address, every other field at its
/// default.
fn valid() -> ServerConfig {
    ServerConfig {
        bind_addresses: vec!["127.0.0.1:8891".parse().unwrap()],
        ..ServerConfig::default()
    }
}

/// For every bounded field: its path, values at its minimum and maximum,
/// and values just outside them where the type can hold such a value.
/// [`set`] gives each field its value: the port of the one bind address,
/// a duration in microseconds, or a number.
const BOUNDS: [(&str, &[u64], &[u64]); 15] = [
    ("bind_addresses", &[1, 65_535], &[0]),
    ("backlog", &[1, 65_535], &[0, 65_536]),
    ("max_connections", &[1, 1_000_000], &[0, 1_000_001]),
    ("timeouts.connect", &[MS, 300 * S], &[999, 300_001 * MS]),
    ("timeouts.read", &[MS, 3_600 * S], &[999, 3_600_001 * MS]),
    ("timeouts.write", &[MS, 3_600 * S], &[999, 3_600_001 * MS]),
    (
        "timeouts.idle",
        &[S, 86_400 * S],
        &[999 * MS, 86_400_001 * MS],
    ),
    (
        "socket_options.recv_buffer_size",
        &[KB, 64 * MB],
        &[KB - 1, 64 * MB + 1],
    ),
    (
        "socket_options.send_buffer_size",
        &[KB, 64 * MB],
        &[KB - 1, 64 * MB + 1],
    ),
    (
        "socket_options.keepalive.time",
        &[S, 86_400 * S],
        &[999 * MS, 86_400_001 * MS],
    ),
    (
        "socket_options.keepalive.interval",
        &[S, 3_600 * S],
        &[999 * MS, 3_600_001 * MS],
    ),
    ("socket_options.keepalive.retries", &[1, 30], &[0, 31]),
    // The minimum is 0 s, which a Duration cannot go below.
    ("shutdown.grace_period", &[0, 60 * S], &[60_001 * MS]),
    (
        "shutdown.drain_timeout",
        &[S, 300 * S],
        &[999 * MS, 300_001 * MS],
    ),
    (
        "resource_limits.per_connection_buffer",
        &[65_540, 16 * MB],
        &[65_539, 16 * MB + 1],
    ),
];

/// Sets the bounded `field` of `config` to `value`, as [`BOUNDS`] reads it.
fn set(field: &str, value: u64, config: &mut ServerConfig) {
    let micros = Duration::from_micros(value);
    match field {
        "bind_addresses" => {
            let port = u16::try_from(value).unwrap();
            config.bind_addresses = vec![SocketAddr::from(([127, 0, 0, 1], port))];
        }
        "backlog" => config.backlog = value.try_into().unwrap(),
        "max_connections" => config.max_connections = value.try_into().unwrap(),
        "timeouts.connect" => config.timeouts.connect = micros,
        "timeouts.read" => config.timeouts.read = micros,
        "timeouts.write" => config.timeouts.write = micros,
        "timeouts.idle" => config.timeouts.idle = micros,
        "socket_options.recv_buffer_size" => {
            config.socket_options.recv_buffer_size = Some(value.try_into().unwrap());
        }
        "socket_options.send_buffer_size" => {
            config.socket_options.send_buffer_size = Some(value.try_into().unwrap());
        }
        "socket_options.keepalive.time" => keepalive(config).time = micros,
        "socket_options.keepalive.interval" => keepalive(config).interval = micros,
        "socket_options.keepalive.retries" => {
            keepalive(config).retries = value.try_into().unwrap();
        }
        "shutdown.grace_period" => config.shutdown.grace_period = micros,
        "shutdown.drain_timeout" => config.shutdown.drain_timeout = micros,
        "resource_limits.per_connection_buffer" => {
            config.resource_limits.per_connection_buffer = value.try_into().unwrap();
        }
        other => panic!("{other} is not a bounded field"),
    }
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
    for (bounded, _, out_of_range) in BOUNDS {
        for (case, value) in out_of_range.iter().enumerate() {
            let mut config = valid();
            set(bounded, *value, &mut config);

            let result = config.validate();

            assert!(
                matches!(&result, Err(ConfigError::OutOfRange { field, .. }) if *field == bounded),
                "{bounded} case {case}: got {result:?}"
            );
        }
    }
}

#[test]
fn cfg_004_accepts_each_bounded_field_at_its_limits() {
    for (bounded, in_range, _) in BOUNDS {
        for (case, value) in in_range.iter().enumerate() {
            let mut config = valid();
            set(bounded, *value, &mut config);

            assert_eq!(config.validate(), Ok(()), "{bounded} case {case}");
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

/// The addresses in `list`.
fn addresses(list: &[&str]) -> Vec<SocketAddr> {
    let mut addresses = Vec::new();
    for address in list {
        addresses.push(address.parse().unwrap());
    }
    addresses
}

#[test]
fn cfg_028_rejects_dual_stack_wildcard_next_to_ipv4_on_the_same_port() {
    let mut config = valid();

    for conflict in [
        ["0.0.0.0:8891", "[::]:8891"],
        ["127.0.0.1:8891", "[::]:8891"],
    ] {
        config.bind_addresses = addresses(&conflict);
        config.socket_options.ipv6_only = false;
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::Inconsistent {
                    rule: "ipv6_only",
                    ..
                })
            ),
            "{conflict:?} with ipv6_only false"
        );

        config.socket_options.ipv6_only = true;
        assert_eq!(
            config.validate(),
            Ok(()),
            "{conflict:?} with ipv6_only true"
        );
    }

    config.socket_options.ipv6_only = false;
    config.bind_addresses = addresses(&["127.0.0.1:8891", "[::1]:8891"]);
    assert_eq!(config.validate(), Ok(()));
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
