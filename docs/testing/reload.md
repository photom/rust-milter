# Test scenarios: Configuration reload

Scenarios for reloading the configuration: the reload coordinator, listener changes, snapshots, the application configuration, errors, and logs. Most are integration tests against a server on leased test ports with a test loader that returns prepared configurations. Every timeout test sets short timeouts and finishes in under 10 seconds. Conventions are in [README.md](README.md).

## Reload coordinator

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-001 | unit | Old `bind_addresses` {A, B} and new {B, C} | the listener sets are compared | C is added, A is removed, and B is kept | [hot-reload: Listener changes](../reference/hot-reload.md#listener-changes) |
| RLD-002 | unit | A coordinator whose loader blocks until the test releases it | five reload requests arrive while the first reload runs | after the release, exactly one more reload runs; the loader is called twice in total | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
| RLD-003 | unit | A coordinator at generation 1 | one reload succeeds, one fails validation, and one succeeds | the generations are 2, 2, and 3 | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-004 | unit | A coordinator in `Loading` | shutdown starts | the reload is abandoned, the loader's result is discarded, and the generation stays the same | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |
| RLD-005 | unit | A coordinator in `Applying`, with a bind the test holds back | shutdown starts, then the test releases the bind | the reload completes and publishes the new generation before the coordinator stops | [hot-reload: Reload coordinator states](../reference/hot-reload.md#reload-coordinator-states) |

## Applying settings

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-010 | integration | A running server at generation 1 and a loader that changes `max_connections` | `reload()` is called | the report has `generation` 2 and `changed` = `["max_connections"]`; `status()` shows `config_generation` 2; an INFO log "Reload applied" lists the name | [hot-reload: Logging and metrics](../reference/hot-reload.md#logging-and-metrics) |
| RLD-011 | integration | A connection open with `timeouts.idle` = 1 s, and a loader that sets it to 3 s | `reload()` is called, and a second connection opens | the first connection closes after about 1 s of idle time, the second after about 3 s | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |
| RLD-012 | integration | Three open connections, `max_connections` = 3, and a loader that sets it to 1 | `reload()` is called, then the fake MTA closes the open connections one by one, trying a new connection after each close | no open connection is closed by the server; new connections are closed at once while one or more remain open, and the first new one after the last close is served | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |
| RLD-013 | integration | A loader that changes `backlog` and `max_connections`, with `bind_addresses` unchanged | `reload()` is called | `changed` lists both; `pending_restart` = `["backlog"]`; a WARN log "Setting needs restart" names `backlog`; the new `max_connections` applies | [hot-reload: What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload) |
| RLD-014 | integration | A loader that changes `socket_options.reuse_port` on a kept address | `reload()` is called | `pending_restart` lists it | [configuration: Settings that need a restart](../reference/configuration.md#settings-that-need-a-restart) |
| RLD-015 | integration | A kept address A with a changed `backlog` pending a restart | one reload removes A, and the next adds it back with the new `backlog` | the second report has no `pending_restart` | [configuration: Reload classes](../reference/configuration.md#reload-classes) |
| RLD-016 | integration | A loader that changes `max_connections` from 1234 to 4321 | `reload()` is called | the logs contain `max_connections` and neither `1234` nor `4321` | [hot-reload: Logging and metrics](../reference/hot-reload.md#logging-and-metrics) |

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

## Errors and triggers

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| RLD-040 | integration | A loader that returns `backlog = 0` | `reload()` is called | it returns `ReloadError::Validation` with `OutOfRange`; the generation is unchanged; an ERROR log "Reload failed" is written | [hot-reload: Errors](../reference/hot-reload.md#errors) |
| RLD-041 | integration | A loader that fails with an I/O error | `reload()` is called | it returns `ReloadError::Load` whose `source()` is the I/O error; the server keeps running | [hot-reload: Errors](../reference/hot-reload.md#errors) |
| RLD-042 | integration | A server without a registered loader | `reload()` is called, then `reload_with_config()` with a changed `max_connections` | the first returns `ReloadError::NoLoader`; the second applies the change | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-043 | integration | A server in a child process with a loader that reads a file the test can change | the test changes `max_connections` in the file and sends `SIGHUP` | the child logs "Reload requested" with `source` = `sighup`, then "Reload applied" with generation 2 | [hot-reload: Reload sequence](../reference/hot-reload.md#reload-sequence) |
| RLD-044 | integration | A server in a child process without a loader | the test sends `SIGHUP` | a WARN log "SIGHUP ignored" has `reason` = `no_loader`, and the child keeps running | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
| RLD-045 | integration | A server that is shutting down | `reload()` is called | it returns `ReloadError::ShuttingDown` | [hot-reload: Concurrency rules](../reference/hot-reload.md#concurrency-rules) |
