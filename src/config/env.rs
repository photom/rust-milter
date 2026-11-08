//! Building and overriding a configuration from environment variables.
//!
//! See "Environment variables" in `docs/reference/configuration.md`.

use std::str::FromStr;

use super::{BindMode, ServerConfig, TraceCommands, text};
use crate::ConfigError;

/// The path of `socket_options.keepalive` in variable names.
const KEEPALIVE: &str = "SOCKET_OPTIONS__KEEPALIVE";

impl ServerConfig {
    /// Builds a configuration from the defaults and every environment
    /// variable that starts with `prefix`, such as `RUST_MILTER_`.
    ///
    /// The helpers check names and value formats only; [`ServerConfig::validate`]
    /// runs the validation rules.
    pub fn from_env(prefix: &str) -> Result<ServerConfig, ConfigError> {
        ServerConfig::from_vars(prefix, process_vars(prefix)?)
    }

    /// Overrides this configuration with the environment variables that start
    /// with `prefix`. On an error the configuration is left unchanged.
    pub fn apply_env(&mut self, prefix: &str) -> Result<(), ConfigError> {
        self.apply_vars(prefix, process_vars(prefix)?)
    }

    /// Like [`ServerConfig::from_env`], but reads the given name and value
    /// pairs instead of the process environment.
    pub fn from_vars<I, K, V>(prefix: &str, vars: I) -> Result<ServerConfig, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut config = ServerConfig::default();
        config.apply_vars(prefix, vars)?;
        Ok(config)
    }

    /// Like [`ServerConfig::apply_env`], with the given name and value pairs.
    ///
    /// Pairs whose name does not start with `prefix` are ignored. A name with
    /// the prefix that names no setting is
    /// [`ConfigError::UnknownEnvVariable`]; a value that cannot be parsed is
    /// [`ConfigError::InvalidEnvValue`], which names the variable but not the
    /// value. On an error the configuration is left unchanged.
    pub fn apply_vars<I, K, V>(&mut self, prefix: &str, vars: I) -> Result<(), ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut next = self.clone();
        let mut keepalive_off = false;
        let mut keepalive_set = false;
        for (name, value) in vars {
            let name = name.as_ref();
            let Some(path) = name.strip_prefix(prefix) else {
                continue;
            };
            match set(&mut next, path, value.as_ref()) {
                Ok(Effect::Set) => {}
                Ok(Effect::KeepaliveOff) => keepalive_off = true,
                Ok(Effect::KeepaliveSet) => keepalive_set = true,
                Err(Failure::Unknown) => {
                    return Err(ConfigError::UnknownEnvVariable(name.to_owned()));
                }
                Err(Failure::Invalid(reason)) => {
                    return Err(ConfigError::InvalidEnvValue {
                        variable: name.to_owned(),
                        reason,
                    });
                }
            }
        }
        if keepalive_off && keepalive_set {
            return Err(ConfigError::InvalidEnvValue {
                variable: format!("{prefix}{KEEPALIVE}"),
                reason: "keepalive is turned off and given settings at once".to_owned(),
            });
        }
        if keepalive_off {
            next.socket_options.keepalive = None;
        }
        *self = next;
        Ok(())
    }
}

/// The process environment's variables that start with `prefix`. A name with
/// the prefix that is not valid UTF-8 names no setting, so it is an error
/// rather than skipped.
fn process_vars(prefix: &str) -> Result<Vec<(String, String)>, ConfigError> {
    let mut vars = Vec::new();
    for (name, value) in std::env::vars_os() {
        if !name.as_encoded_bytes().starts_with(prefix.as_bytes()) {
            continue;
        }
        let Some(name) = name.to_str() else {
            return Err(ConfigError::UnknownEnvVariable(
                name.to_string_lossy().into_owned(),
            ));
        };
        let Ok(value) = value.into_string() else {
            return Err(ConfigError::InvalidEnvValue {
                variable: name.to_owned(),
                reason: "the value is not valid UTF-8".to_owned(),
            });
        };
        vars.push((name.to_owned(), value));
    }
    Ok(vars)
}

/// What setting a variable did.
enum Effect {
    Set,
    KeepaliveOff,
    KeepaliveSet,
}

/// Why a variable could not be applied.
enum Failure {
    Unknown,
    Invalid(String),
}

impl From<String> for Failure {
    fn from(reason: String) -> Failure {
        Failure::Invalid(reason)
    }
}

/// Sets the setting at `path`, the variable name after the prefix.
fn set(config: &mut ServerConfig, path: &str, value: &str) -> Result<Effect, Failure> {
    let socket = &mut config.socket_options;
    let limits = &mut config.resource_limits;
    match path {
        "BIND_ADDRESSES" => config.bind_addresses = bind_addresses(value)?,
        "BIND_MODE" => config.bind_mode = bind_mode(value)?,
        "BACKLOG" => config.backlog = number(value)?,
        "MAX_CONNECTIONS" => config.max_connections = number(value)?,
        "TIMEOUTS__CONNECT" => config.timeouts.connect = duration(value)?,
        "TIMEOUTS__READ" => config.timeouts.read = duration(value)?,
        "TIMEOUTS__WRITE" => config.timeouts.write = duration(value)?,
        "TIMEOUTS__IDLE" => config.timeouts.idle = duration(value)?,
        "SOCKET_OPTIONS__TCP_NODELAY" => socket.tcp_nodelay = boolean(value)?,
        KEEPALIVE => {
            if value != "false" {
                return Err(Failure::Invalid(
                    "only false is accepted; the KEEPALIVE__* variables set its parts".to_owned(),
                ));
            }
            return Ok(Effect::KeepaliveOff);
        }
        "SOCKET_OPTIONS__KEEPALIVE__TIME" => {
            socket.keepalive.get_or_insert_default().time = duration(value)?;
            return Ok(Effect::KeepaliveSet);
        }
        "SOCKET_OPTIONS__KEEPALIVE__INTERVAL" => {
            socket.keepalive.get_or_insert_default().interval = duration(value)?;
            return Ok(Effect::KeepaliveSet);
        }
        "SOCKET_OPTIONS__KEEPALIVE__RETRIES" => {
            socket.keepalive.get_or_insert_default().retries = number(value)?;
            return Ok(Effect::KeepaliveSet);
        }
        "SOCKET_OPTIONS__RECV_BUFFER_SIZE" => socket.recv_buffer_size = Some(number(value)?),
        "SOCKET_OPTIONS__SEND_BUFFER_SIZE" => socket.send_buffer_size = Some(number(value)?),
        "SOCKET_OPTIONS__REUSE_ADDR" => socket.reuse_addr = boolean(value)?,
        "SOCKET_OPTIONS__REUSE_PORT" => socket.reuse_port = boolean(value)?,
        "SOCKET_OPTIONS__IPV6_ONLY" => socket.ipv6_only = boolean(value)?,
        "SHUTDOWN__GRACE_PERIOD" => config.shutdown.grace_period = duration(value)?,
        "SHUTDOWN__DRAIN_TIMEOUT" => config.shutdown.drain_timeout = duration(value)?,
        "RESOURCE_LIMITS__MAX_FDS" => limits.max_fds = Some(number(value)?),
        "RESOURCE_LIMITS__MAX_MEMORY" => limits.max_memory = Some(number(value)?),
        "RESOURCE_LIMITS__PER_CONNECTION_BUFFER" => limits.per_connection_buffer = number(value)?,
        "TELEMETRY__TRACE_COMMANDS" => config.telemetry.trace_commands = trace_commands(value)?,
        _ => return Err(Failure::Unknown),
    }
    Ok(Effect::Set)
}

/// Comma-separated addresses; spaces around items are ignored.
fn bind_addresses(value: &str) -> Result<Vec<std::net::SocketAddr>, String> {
    let mut addresses = Vec::new();
    for (index, item) in value.split(',').enumerate() {
        match ServerConfig::parse_bind_address(item.trim()) {
            Ok(address) => addresses.push(address),
            Err(error) => return Err(format!("address {}: {error}", index + 1)),
        }
    }
    Ok(addresses)
}

fn bind_mode(value: &str) -> Result<BindMode, String> {
    match value {
        "fail_fast" => Ok(BindMode::FailFast),
        "best_effort" => Ok(BindMode::BestEffort),
        _ => Err("expected fail_fast or best_effort".to_owned()),
    }
}

fn trace_commands(value: &str) -> Result<TraceCommands, String> {
    match value {
        "none" => Ok(TraceCommands::None),
        "envelope" => Ok(TraceCommands::Envelope),
        "all" => Ok(TraceCommands::All),
        _ => Err("expected none, envelope, or all".to_owned()),
    }
}

fn boolean(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("expected true or false".to_owned()),
    }
}

/// A whole number. The parse error's text never contains the value.
fn number<T: FromStr<Err = std::num::ParseIntError>>(value: &str) -> Result<T, String> {
    match value.parse() {
        Ok(number) => Ok(number),
        Err(error) => Err(format!("expected a whole number: {error}")),
    }
}

fn duration(value: &str) -> Result<std::time::Duration, String> {
    match text::parse_duration(value) {
        Ok(duration) => Ok(duration),
        Err(reason) => Err(reason.to_owned()),
    }
}
