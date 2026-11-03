# Test scenarios: Configuration

Scenarios for configuration validation, address parsing, and the serde file format. Reload behavior gets its own file later (`RLD`). Conventions are in [README.md](README.md).

## Validation

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CFG-001 | unit | `ServerConfig::default()` with no bind address | it is validated | `MissingBindAddress` | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-002 | unit | A configuration with one bind address and all other fields at their defaults | it is validated | it is valid | [configuration: ServerConfig reference](../reference/configuration.md#serverconfig-reference) |
| CFG-003 | unit | For every bounded field, one value below its minimum and one above its maximum, all others valid | each configuration is validated | `OutOfRange` naming that field, for every case | [configuration: ServerConfig reference](../reference/configuration.md#serverconfig-reference) |
| CFG-004 | unit | For every bounded field, its minimum and its maximum | each configuration is validated | it is valid | [configuration: ServerConfig reference](../reference/configuration.md#serverconfig-reference) |
| CFG-005 | unit | The same bind address listed twice | it is validated | `DuplicateBindAddress` with that address | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-006 | unit | `per_connection_buffer` of 65,539 and of 65,540 | each is validated | the first fails, the second is valid | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-007 | unit | Two listeners, `max_connections` 1,000, and `max_fds` 1,011 or 1,012 | each is validated | 1,011 fails with `Inconsistent`, 1,012 is valid | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-008 | unit | `max_memory` smaller than `per_connection_buffer` | it is validated | `Inconsistent` | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-009 | unit | `reuse_port` true on a platform without `SO_REUSEPORT` (skipped where it is supported) | it is validated | `Unsupported` naming `reuse_port` | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-010 | integration | An invalid configuration and a leased test port | `build()` is called | an error is returned, and the port can still be bound by the test, so no socket was opened | [api: Server](../reference/api-specification.md#server) |
| CFG-028 | unit | `0.0.0.0:8891` and `[::]:8891` with `ipv6_only` false, the same with `127.0.0.1:8891`, and both with `ipv6_only` true | each is validated | the first two fail with `Inconsistent` naming `ipv6_only`; with `ipv6_only` true they are valid, and so is `[::1]:8891` next to `127.0.0.1:8891` | [configuration: Validation rules](../reference/configuration.md#validation-rules) |

## Address parsing

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CFG-020 | unit | `127.0.0.1:8891`, `[::1]:8891`, `inet:8891@127.0.0.1`, `inet6:8891@::1` | each is parsed | the matching socket address | [configuration: Top level](../reference/configuration.md#top-level) |
| CFG-021 | unit | `unix:/run/milter.sock` and `local:/run/milter.sock` | each is parsed | `InvalidFormat` | [configuration: Top level](../reference/configuration.md#top-level) |
| CFG-022 | unit | Ports 0 and 70000 | each is parsed | `InvalidPort` | [configuration: Top level](../reference/configuration.md#top-level) |
| CFG-023 | unit | `inet:8891@not a host` | it is parsed | `InvalidHost` | [api: ConfigError](../reference/api-specification.md#configerror) |
| CFG-024 | unit | `inet:8891@::1` and `inet6:8891@127.0.0.1`, a host of the other family | each is parsed | `InvalidHost` | [configuration: Top level](../reference/configuration.md#top-level) |
| CFG-025 | unit | `inet:8891` and `inet6:8891`, libmilter's forms without a host | each is parsed | `0.0.0.0:8891` and `[::]:8891`, every address of the family | [configuration: Top level](../reference/configuration.md#top-level) |
| CFG-026 | unit | `[fe80::1%2]:8891`, an IPv6 address with a scope ID | it is parsed | the address with scope ID 2, so text written from such an address reads back | [configuration: Top level](../reference/configuration.md#top-level) |
| CFG-027 | unit | `inet:127.0.0.1:8891`, Postfix's form, and `inet6:8891@fe80::1%2` | each is parsed | `InvalidFormat` for the first, so the error names the form rather than the port; the second gives the address with scope ID 2 | [configuration: Top level](../reference/configuration.md#top-level) |

## File format (serde feature)

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CFG-040 | unit | A TOML document with only `bind_addresses` | it is deserialized | every other field has its default | [configuration: Compatibility rules](../reference/configuration.md#compatibility-rules) |
| CFG-041 | unit | A TOML document with the unknown key `max_conections` | it is deserialized | an error that names `max_conections` | [configuration: Compatibility rules](../reference/configuration.md#compatibility-rules) |
| CFG-042 | unit | The durations `"500ms"`, `"30s"`, `"5m"`, `"1h"` | each is deserialized | 0.5 s, 30 s, 300 s, 3,600 s | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-043 | unit | The durations `30`, `"30"`, and `"30sec"` | each is deserialized | an error for each | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-044 | unit | `bind_mode = "best_effort"`, `bind_mode = "BestEffort"` | each is deserialized | the first gives `BestEffort`, the second fails | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-045 | unit | `trace_commands = "all"` inside `[telemetry]` | it is deserialized | `TraceCommands::All` | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-046 | unit | `[socket_options]` with `keepalive = false`, and with a `[socket_options.keepalive]` table | each is deserialized | `None`, and `Some` with the table's values and defaults for the rest | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-047 | unit | Nested tables `[timeouts]`, `[shutdown]`, `[resource_limits]` | they are deserialized | the values land in the matching parts | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-048 | unit | A valid configuration with non-default values and keepalive off, and one with the defaults and keepalive on | each is serialized to TOML and read back | the result equals the original, and the TOML lists every field, defaults included; keepalive off is written as `false` | [configuration: File format](../reference/configuration.md#file-format-serde-feature) |
| CFG-049 | unit | A deserialized configuration with an invalid value, such as `backlog = 0` | it is validated | `OutOfRange`; deserializing alone does not validate | [configuration: Validation rules](../reference/configuration.md#validation-rules) |
| CFG-050 | unit | `[socket_options]` with `keepalive = true` | it is deserialized | an error; the defaults come from leaving the key out | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-051 | unit | A configuration whose `timeouts.read` is 1.5 ms | it is serialized | an error, because the file format has no unit below a millisecond | [configuration: Mapping](../reference/configuration.md#mapping) |
| CFG-052 | unit | A configuration whose bind address is `[fe80::1%2]:8891` | it is serialized to TOML and read back | the result equals the original, scope ID included | [configuration: Mapping](../reference/configuration.md#mapping) |

## Reload classes

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CFG-060 | unit | The reload class the code assigns to every `ServerConfig` field | it is compared with the tables in configuration.md | every field has the class the document gives it | [configuration: Reload classes](../reference/configuration.md#reload-classes) |

## Environment variables

These tests use `from_vars` and `apply_vars`, so they never change the real process environment. CFG-094 and CFG-095 are the exceptions: only the process environment can hold text that is not valid UTF-8, so each runs its test binary again with the variable set in the child process's environment.

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| CFG-080 | unit | The pair `RUST_MILTER_BIND_ADDRESSES` = `0.0.0.0:8891, [::]:8891` and nothing else | `from_vars` runs with the prefix `RUST_MILTER_` | two bind addresses, and every other field at its default | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-081 | unit | A configuration with `max_connections` 1,000 and `backlog` 2,048, and the pair `RUST_MILTER_MAX_CONNECTIONS` = `500` | `apply_vars` runs | `max_connections` is 500 and `backlog` stays 2,048 | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-082 | unit | The pairs `RUST_MILTER_TIMEOUTS__IDLE` = `120s` and `RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE__TIME` = `30s` | `from_vars` runs | `timeouts.idle` is 120 s and `socket_options.keepalive.time` is 30 s | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-083 | unit | The pair `RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE` = `false` | `from_vars` runs | `socket_options.keepalive` is `None` | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-084 | unit | The pair `RUST_MILTER_MAX_CONECTIONS` = `500` | `from_vars` runs | `UnknownEnvVariable` naming `RUST_MILTER_MAX_CONECTIONS` | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-085 | unit | The pair `RUST_MILTER_BACKLOG` = `many` | `from_vars` runs | `InvalidEnvValue` naming `RUST_MILTER_BACKLOG`, and the error text does not contain `many` | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-086 | unit | The pairs `MYFILTER_MAX_CONNECTIONS` = `200` and `RUST_MILTER_MAX_CONNECTIONS` = `300` | `from_vars` runs with the prefix `MYFILTER_` | `max_connections` is 200; the `RUST_MILTER_` variable is ignored | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-087 | unit | The pairs `PATH` and `OTEL_EXPORTER_OTLP_ENDPOINT` with any values | `from_vars` runs with the prefix `RUST_MILTER_` | they are ignored, and the result equals the defaults | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-088 | unit | The pairs `RUST_MILTER_BIND_MODE` = `best_effort`, and in another run `BestEffort` | `from_vars` runs | the first gives `BestEffort`, the second is `InvalidEnvValue`; the formats match the file format (CFG-044) | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-089 | unit | The pair `RUST_MILTER_BACKLOG` = `0` | `from_vars` runs, then the result is validated | `from_vars` succeeds; validation fails with `OutOfRange`, because the helpers check formats only | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-090 | unit | One test per setting in the reference tables | a variable is built from the setting's path by the naming rule | `from_vars` sets exactly that setting, so every setting has a working variable | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-091 | unit | The pairs `RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE` = `false` and `RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE__TIME` = `30s` | `from_vars` runs | `InvalidEnvValue` naming `RUST_MILTER_SOCKET_OPTIONS__KEEPALIVE`: keepalive cannot be off and set at once, and the variables have no order to decide by | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-092 | unit | A configuration with `max_connections` 1,000, and the pairs `RUST_MILTER_MAX_CONNECTIONS` = `500` and `RUST_MILTER_BACKLOG` = `many` | `apply_vars` runs | `InvalidEnvValue`, and `max_connections` stays 1,000: a failed apply changes nothing | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-093 | unit | The pair `RUST_MILTER_max_connections` = `500`, the setting's path in lower case | `from_vars` runs | `UnknownEnvVariable`; variable names are in upper case | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-094 | integration | A process environment variable whose name is `CFG094_` followed by the byte `0xFF`, which is not valid UTF-8 | `from_env` runs with the prefix `CFG094_` | `UnknownEnvVariable` with the name, the invalid byte shown as U+FFFD; a name with the prefix is never skipped | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
| CFG-095 | integration | The process environment variable `CFG095_BACKLOG` with a value that is not valid UTF-8 | `from_env` runs with the prefix `CFG095_` | `InvalidEnvValue` naming `CFG095_BACKLOG` | [configuration: Environment variables](../reference/configuration.md#environment-variables) |
