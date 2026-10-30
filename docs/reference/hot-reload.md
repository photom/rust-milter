# Configuration hot reload specification

Status: design, not implemented.

Reloading the server configuration while rust-milter keeps running, triggered by `SIGHUP` or by an API call, without dropping MTA connections.

## Contents

- [Purpose and scope](#purpose-and-scope)
- [Architecture](#architecture)
- [Reload sequence](#reload-sequence)
- [Reload coordinator states](#reload-coordinator-states)
- [What each setting does on reload](#what-each-setting-does-on-reload)
- [Listener changes](#listener-changes)
- [Application configuration](#application-configuration)
- [Concurrency rules](#concurrency-rules)
- [API](#api)
- [Errors](#errors)
- [Logging and metrics](#logging-and-metrics)
- [Testing requirements](#testing-requirements)
- [Open questions](#open-questions)

## Purpose and scope

An operator changes a timeout, a connection limit, or a listen address, and sends `SIGHUP`. Restarting the milter would drop every open MTA connection, and each MTA would apply its failure policy to the messages in flight (Sendmail `F=` flags, Postfix `milter_default_action`). A reload avoids that.

rust-milter is a library. It does not know where the application keeps its configuration, so it never reads a file itself. The application registers a loader. On `SIGHUP` the server calls the loader, validates the result, and applies it.

In scope:
- reloading `ServerConfig` (see `docs/reference/configuration.md`)
- reloading the application's own configuration for its handlers, such as spam thresholds or allow lists
- adding and removing listeners

Out of scope:
- settings fixed when the Tokio runtime starts, such as `worker_threads`. They need a restart.
- replacing the handler code. That needs a new binary and a restart.
- reloading the log filter. The application owns its `tracing` subscriber and can reload it from the same loader.

## Architecture

```mermaid
flowchart LR
    sighup["SIGHUP"]
    api["ServerHandle::reload()"]
    subgraph server["rust-milter server"]
        direction TB
        coord["Reload coordinator<br/>(one reload at a time)"]
        loader["ConfigLoader<br/>(registered by the application)"]
        validate["Validation"]
        watch["Config channel<br/>(tokio::sync::watch,<br/>generation number)"]
        acceptor["Accept loop and listeners"]
        conns["Connection tasks<br/>(config snapshot taken at accept)"]
        handlers["Handlers<br/>(read the application config<br/>at session boundaries)"]
    end
    sighup --> coord
    api --> coord
    coord --> loader
    loader --> validate
    validate --> watch
    watch --> acceptor
    acceptor --> conns
    watch --> handlers
```

Responsibilities:
- The reload coordinator receives reload requests, runs one at a time, and reports the result.
- The `ConfigLoader` belongs to the application. It returns a new `ServerConfig` and a new application configuration, or an error.
- Validation runs the same checks as startup, plus the reload-specific checks below.
- The config channel publishes each accepted configuration as an immutable `Arc` with an increasing generation number.
- The accept loop reads the current configuration for every accepted connection. Each connection task keeps the snapshot it started with.
- Handlers read the current application configuration when a new SMTP session or message starts.

## Reload sequence

```mermaid
sequenceDiagram
    participant Op as Operator
    participant Coord as Reload coordinator
    participant Loader as ConfigLoader
    participant Acc as Accept loop
    participant Ch as Config channel
    participant Conn as Existing connections

    Op->>Coord: SIGHUP
    Coord->>Loader: load()
    Loader-->>Coord: ServerConfig + application config
    Coord->>Coord: validate (startup checks + reload checks)
    alt validation fails
        Coord-->>Op: log ERROR, keep generation N
    else validation passes
        Coord->>Acc: bind added listeners
        alt a bind fails in FailFast mode
            Acc-->>Coord: bind error
            Coord->>Acc: close the listeners bound by this reload
            Coord-->>Op: log ERROR, keep generation N
        else all binds succeed
            Coord->>Ch: publish generation N+1
            Coord->>Acc: stop removed listeners
            Note over Conn: keep running with the snapshot of generation N
            Note over Acc: new connections get generation N+1
            Coord-->>Op: log INFO with changed field names
        end
    end
```

The order matters. New listeners are bound before the new configuration is published, and removed listeners stop only after it. Moving the server to a new address therefore never leaves a moment without a listener, and a failed bind leaves the running configuration untouched.

## Reload coordinator states

```mermaid
stateDiagram-v2
    [*] --> Ready
    Ready --> Loading: SIGHUP or reload()
    Loading --> Validating: loader returned a configuration
    Loading --> Ready: loader error (generation unchanged)
    Validating --> Applying: validation passed
    Validating --> Ready: validation failed (generation unchanged)
    Applying --> Ready: published generation N+1
    Applying --> Ready: bind failed, rolled back (generation unchanged)
    Ready --> Stopped: shutdown started
    Loading --> Stopped: shutdown started (reload abandoned)
    Validating --> Stopped: shutdown started (reload abandoned)
    Stopped --> [*]
```

A request that arrives while a reload is running is not lost. The coordinator records it, and when the current reload ends it runs one more reload. Any number of requests that arrive in the meantime cause one extra reload only, which then picks up the latest configuration.

`Applying` is not interrupted by shutdown. It is short (binding sockets and publishing a value), and stopping halfway would leave listeners in an unknown state.

## What each setting does on reload

Every connection keeps the configuration snapshot it was accepted with until it closes. One milter connection lives for one SMTP session with Sendmail and Postfix, so every open connection picks up the new values within one SMTP session.

Which settings apply at once, which apply to new connections or new listeners, and which need a restart is specified per setting in [configuration.md](configuration.md#reload-classes).

A change to a setting that needs a restart does not fail the reload. The server applies everything else, keeps the running value for that setting, and lists it as `pending_restart` in the log and in the reload report.

## Listener changes

The coordinator compares the old and new `bind_addresses` as sets:

| Case | Action |
|------|--------|
| Address only in the new set | Bind a new listener before publishing |
| Address only in the old set | Stop accepting on it after publishing. Open connections accepted on it keep running |
| Address in both sets | Keep the listener as it is |

A bind failure follows `bind_mode` from the new configuration (see `docs/reference/configuration.md`):
- `FailFast`: close every listener this reload bound, keep the running configuration, and report the error.
- `BestEffort`: publish the new configuration with the listeners that did bind, and report the failed addresses as a warning.

A reload never leaves the server with no listener. If the new set would bind nothing, the reload fails.

## Application configuration

Handlers often need their own settings. The server carries them next to `ServerConfig`, as a type chosen by the application: the application passes the first value to `ServerBuilder::app_config` and returns later values from its reload loader. The [reload tutorial](../tutorials/reloading-configuration.md) shows a complete loader.

Handlers read the application configuration through a `ConfigWatch<A>`, a thin wrapper around `tokio::sync::watch::Receiver<Arc<A>>`. `ConfigWatch::current()` returns the current `Arc<A>` and does not block.

Read it once per SMTP session (at `SMFIC_CONNECT`) or once per message (at `SMFIC_MAIL`), and keep that `Arc` until the session or message ends. A message then sees one consistent configuration, even if a reload happens halfway through it.

The application configuration follows the same isolation rule as the rest of rust-milter: it holds settings, never data from an SMTP session.

## Concurrency rules

1. One reload runs at a time. The coordinator owns the listener set; no other task changes it.
2. Readers never wait for a reload. The accept loop and the handlers read the current `Arc` and continue.
3. A published configuration is immutable. A reload publishes a new `Arc`; it never changes the old one in place.
4. The generation number grows by one per published configuration and never goes back. A failed reload does not change it.
5. Once shutdown has started, `reload()` returns `ShuttingDown` and `SIGHUP` is logged and ignored.
6. Without a registered loader, `SIGHUP` is logged at WARN level and ignored. `reload()` with an explicit configuration still works.

## API

The reload API (`ServerBuilder::reload_with`, `ServerHandle::reload`, `reload_with_config`, `config_watch`, `config_generation`) and the `ReloadReport` and `ReloadError` types are specified in [api-specification.md](api-specification.md#configuration-and-reload-api).

## Errors

The reload errors are the variants of `ReloadError` in [api-specification.md](api-specification.md#reloaderror). Every one of them leaves the server running with the previous generation, and none changes the exit code.

## Logging and metrics

| Event | Level | Fields |
|-------|-------|--------|
| Reload requested | INFO | `source` (`sighup` or `api`), `generation` |
| Reload coalesced | DEBUG | `pending_requests` |
| Reload applied | INFO | `generation`, `changed`, `pending_restart`, `listeners_added`, `listeners_removed` |
| Reload failed | ERROR | `generation`, `error` |
| Setting needs restart | WARN | `setting` |
| SIGHUP ignored | WARN | `reason` (`no_loader` or `shutting_down`) |

Logs carry the names of changed settings, never their values. A configuration can contain secrets, and the application configuration is opaque to the library.

The reload metrics are defined under "Configuration reload metrics" in [telemetry.md](telemetry.md).

The health check gains two fields: `config_generation` and `pending_restart`.

## Testing requirements

Unit tests:
- the listener set comparison for added, removed, and kept addresses
- classification of every `ServerConfig` field (at once, new connections, new listeners, restart)
- coalescing: several requests during one reload cause exactly one more reload
- generation numbering on success and failure

Integration tests:
- a reload during an open SMTP session: the session finishes with its old timeouts, and the next connection uses the new ones
- moving the listen address: no moment without a listener, and connections on the old address finish
- a bind failure in `FailFast` mode rolls back; in `BestEffort` mode it applies the rest
- an invalid configuration leaves the server unchanged
- `SIGHUP` without a loader, and during shutdown
- a handler reading `ConfigWatch` sees one value for a whole message while a reload happens

## Open questions

1. Should a lowered `max_connections` also close idle connections above the new limit, or only stop new ones as designed?
2. Should a changed `backlog` on an unchanged address rebind the listener with `SO_REUSEPORT`, so it applies without a restart?
3. Does the milter client in `docs/reference/milter-client.md` need the same reload support for its pool settings?
