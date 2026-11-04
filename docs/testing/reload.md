# Test scenarios: Configuration reload

Scenarios for reloading the configuration: the reload coordinator, listener changes, snapshots, the application configuration, errors, and logs. Most are integration tests against a server on leased test ports with a test loader that returns prepared configurations. Every timeout test sets short timeouts and finishes in under 10 seconds. Conventions are in [README.md](README.md).

## Reload coordinator

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-001 | unit | Old `bind_addresses` {A, B} and new {B, C} | the listener sets are compared | C is added, A is removed, and B is kept | [hot-reload: Listener changes](../reference/hot-reload.md#listener-changes) |
| RLD-002 | unit | A coordinator whose loader blocks until the test releases it | five reload requests arrive while the first reload runs | after the release, exactly one more reload runs; the loader is called twice in total. The first caller gets the report of the first reload, and the other five the report of the second | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
| RLD-003 | unit | A coordinator at generation 1 | one reload succeeds, one fails validation, and one succeeds | the generations are 2, 2, and 3 | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-004 | unit | A coordinator in `Loading` | shutdown starts | the reload is abandoned, the loader's result is discarded, and the generation stays the same | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
| RLD-005 | unit | A coordinator in `Applying`, with a bind the test holds back | shutdown starts, then the test releases the bind | the reload completes and publishes the new generation before the coordinator stops | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
| RLD-006 | unit | Two configurations that differ in `max_connections`, `timeouts.idle`, and keepalive `time`, and a third that turns keepalive off | the first two are compared, then the first and the third | the names are `max_connections`, `timeouts.idle`, and `socket_options.keepalive.time`, in the order of [configuration.md](../reference/configuration.md#serverconfig-reference); then `socket_options.keepalive` alone | [hot-reload: Logging and metrics](../reference/hot-reload.md#logging-and-metrics) |

## Applying settings

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-010 | integration | A running server at generation 1 and a loader that changes `max_connections` | `reload()` is called | the report has `generation` 2 and `changed` = `["max_connections"]`; `status()` shows `config_generation` 2; an INFO log "Reload applied" lists the name | [hot-reload: Logging and metrics](../reference/hot-reload.md#logging-and-metrics) |
| RLD-011 | integration | A connection open with `timeouts.idle` = 1 s, and a loader that sets it to 3 s | `reload()` is called, and a second connection opens | the first connection closes after about 1 s of idle time, the second after about 3 s | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |
| RLD-012 | integration | Three open connections, `max_connections` = 3, and a loader that sets it to 1 | `reload()` is called, then the fake MTA closes the open connections one by one, trying a new connection after each close | no open connection is closed by the server; new connections are closed at once while one or more remain open, and the first new one after the last close is served | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |
| RLD-013 | integration | `max_connections` = 2, and a loader that changes `backlog` and sets `max_connections` to 1, with `bind_addresses` unchanged | `reload()` is called, then two connections open | it returns `ReloadError::RestartRequired` with `settings` = `["backlog"]` and the message "changing backlog requires a restart; nothing was applied"; an ERROR log "Reload failed" carries that message; the generation stays 1, and both connections are served because `max_connections` is still 2 | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |
| RLD-014 | integration | A server on A, and a loader that adds B and sets `socket_options.reuse_port` to true | `reload()` is called | it returns `RestartRequired` naming `socket_options.reuse_port`; B refuses connections, and `status()` lists only A | [configuration: Reload classes](../reference/configuration.md#reload-classes) |
| RLD-015 | integration | A loader that first changes `backlog`, `reuse_addr`, `reuse_port`, `ipv6_only`, and `max_connections`, then returns the running values of the four with the new `max_connections` | `reload()` is called twice | the first returns `RestartRequired` with `settings` = `["backlog", "socket_options.reuse_addr", "socket_options.reuse_port", "socket_options.ipv6_only"]`, the order of [configuration.md](../reference/configuration.md#serverconfig-reference); the second applies with `generation` 2 and `changed` = `["max_connections"]` | [api: ReloadError](../reference/api-specification.md#reloaderror) |
| RLD-016 | integration | A loader that changes `max_connections` from 777777 to 888888, numbers too large for a port | `reload()` is called | the logs contain `max_connections` and neither `777777` nor `888888` | [hot-reload: Logging and metrics](../reference/hot-reload.md#logging-and-metrics) |
| RLD-017 | integration | Default socket options, a connection open after HELO, and a loader that sets keepalive `time` to 120 s, `interval` to 20 s, and `retries` to 3 | `reload()` is called, and a second connection opens | the first connection's socket keeps 60 s, 10 s, and 6 retries, and its `ConnectionInfo` has `config_generation` 1; the second connection's socket has 120 s, 20 s, and 3, and `config_generation` 2 | [configuration: Reload classes](../reference/configuration.md#reload-classes) |
| RLD-018 | integration | A recording server, a fresh connection brought to each state in turn: just accepted, after option negotiation, `ST_CONN`, `ST_HELO`, `ST_MAIL`, `ST_RCPT`, `ST_DATA`, `ST_HDRS`, `ST_EOHS`, `ST_BODY`, `ST_ENDM`, and `ST_UNKN`, and a loader that changes `max_connections`, `timeouts.idle`, and keepalive `time` | `reload()` is called | no connection closes or sees a callback it did not ask for; each one answers its next command, and its socket keeps the keepalive settings it was accepted with | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |

## Settings read when used

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-050 | integration | A server without `max_memory`, and a loader that sets `max_memory` and `per_connection_buffer` to 65,540 bytes | `reload()` is called, then 17 connections open and negotiate one after another | the first 16 are served and the 17th is closed at once, because each connection reserves 4 KB | [configuration: Settings that do not need a restart](../reference/configuration.md#settings-that-do-not-need-a-restart) |
| RLD-051 | integration | A server with `grace_period` = 0, a connection open after HELO, and a loader that sets `grace_period` to 500 ms | `reload()` is called, then `shutdown()` | the connection is still open 200 ms after `shutdown()` and closes about 500 ms after it: the shutdown reads the timings of the current generation | [configuration: Settings that do not need a restart](../reference/configuration.md#settings-that-do-not-need-a-restart) |

## Listener changes

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-020 | integration | A server on leased port A, a connection open on A, and a loader that moves `bind_addresses` to leased port B | a probe connects in a loop to A and B while `reload()` runs | at every moment at least one of A and B accepts; afterwards A refuses new connections, and the open connection on A keeps working until it closes | [hot-reload: Listener changes](../reference/hot-reload.md#listener-changes) |
| RLD-021 | integration | `FailFast`, a server on A, and a loader that adds B and C, where C is already bound by the test | `reload()` is called | it returns `ReloadError::Bind` for C; B is closed again; the generation is unchanged and A keeps serving | [hot-reload: Listener changes](../reference/hot-reload.md#listener-changes) |
| RLD-022 | integration | `BestEffort`, a server on A, and a loader that adds B and C, where C is already bound | `reload()` is called | the new generation is published with A and B; `bind_failures` lists C, logged as a warning | [hot-reload: Listener changes](../reference/hot-reload.md#listener-changes) |
| RLD-023 | integration | `BestEffort`, a server on A, and a loader that replaces A with C, where C is already bound | `reload()` is called | it returns `ReloadError::NoListener`; the generation is unchanged and A keeps serving | [hot-reload: Listener changes](../reference/hot-reload.md#listener-changes) |

## Application configuration

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-030 | integration | A handler that reads `ConfigWatch` in `mail` and records the value in `end_of_message` | a reload publishes a new value between MAIL and end of message, and a second message follows | the first message records the old value, the second the new one | [hot-reload: Application configuration](../reference/hot-reload.md#application-configuration) |
| RLD-031 | integration | A task waiting on `ConfigWatch::changed()` | a reload succeeds | the wait completes, and `current()` returns the new value with the new `generation()` | [api: Configuration and reload API](../reference/api-specification.md#configuration-and-reload-api) |
| RLD-032 | integration | A loader that returns a valid `ServerConfig` and an application value that its own check rejects, as an error | `reload()` is called | it returns `ReloadError::Load`; both configurations stay at the current generation | [configuration: Application configuration](../reference/configuration.md#application-configuration) |
| RLD-033 | integration | A task waiting on `ConfigWatch::changed()`, and a loader that returns a new application value with a `ServerConfig` that changes `backlog` | `reload()` is called | it returns `RestartRequired`; 100 ms later the wait has not completed, and `current()` still returns the old value | [hot-reload: Reload sequence](../reference/hot-reload.md#reload-sequence) |

## Errors and triggers

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-040 | integration | A loader that returns `backlog = 0` | `reload()` is called | it returns `ReloadError::Validation` with `OutOfRange`; the generation is unchanged; an ERROR log "Reload failed" is written | [hot-reload: Errors](../reference/hot-reload.md#errors) |
| RLD-041 | integration | A loader that fails with an I/O error | `reload()` is called | it returns `ReloadError::Load` whose `source()` is the I/O error; the server keeps running | [hot-reload: Errors](../reference/hot-reload.md#errors) |
| RLD-042 | integration | A server without a registered loader | `reload()` is called, then `reload_with_config()` with a changed `max_connections` | the first returns `ReloadError::NoLoader`; the second applies the change | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-043 | integration | A server in a child process with a loader that reads a file the test can change | the test changes `max_connections` in the file and sends `SIGHUP` | the child logs "Reload requested" with `source` = `sighup`, then "Reload applied" with generation 2 | [hot-reload: Reload sequence](../reference/hot-reload.md#reload-sequence) |
| RLD-044 | integration | A server in a child process without a loader | the test sends `SIGHUP` | a WARN log "SIGHUP ignored" has `reason` = `no_loader`, and the child keeps running | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-045 | integration | A server that is shutting down | `reload()` is called | it returns `ReloadError::ShuttingDown` | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-046 | integration | A server that is built but not running | `reload_with_config()` is called with a changed `max_connections`, then `run()` | the reload waits until `run()` has bound the listeners, then returns a report with `generation` 2 | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
