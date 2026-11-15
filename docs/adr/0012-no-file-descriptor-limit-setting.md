# ADR 0012: rust-milter has no file descriptor limit setting

- Status: Accepted
- Date: 2026-11-08
- Affects: `ServerConfig`, `ResourceKind`, the configuration reference, the environment variables, the file format

## Context

The configuration reference described `resource_limits.max_fds` as "the most file descriptors the server uses", checked at accept time, and the API specification listed `ResourceLimit(MaxFds)` as the error a connection got at that limit. The code only validated the setting: a configuration had to set it to at least `max_connections` plus the listeners plus 10. No code counted descriptors or created `ResourceKind::MaxFds`.

An accept-time check could not fire either. The server holds one descriptor per connection and one per listener, and validation already left room for `max_connections` of them plus 10 spare, so `max_connections` stops a new connection first.

Only the process limit (`RLIMIT_NOFILE`, set with `ulimit -n` or `LimitNOFILE=`) can run out, and it also counts the descriptors the application opens. When it runs out, `accept()` fails with `EMFILE`, and the accept loop backs off and keeps running.

## Options considered

- (a) Remove the setting.
- (b) Enforce it at accept: drop the validation rule and close a new connection once connections plus listeners plus 10 reach `max_fds`. This makes it a second connection cap.
- (c) Keep it as a consistency check against `max_connections`, and document that it never refuses a connection.

## Decision

Option (a). `ServerConfig` loses `resource_limits.max_fds`, `ResourceKind` loses `MaxFds`, and the environment variable `RESOURCE_LIMITS__MAX_FDS` and the validation rule go with them. Operators size the process limit as the production guide describes: at least `max_connections` plus the listeners plus 10.

## Consequences

Positive:
- Each setting does what its documentation says.
- Operators have one setting fewer to keep consistent with `max_connections`.

Negative:
- A configuration file that sets `max_fds` fails to load, because unknown keys are errors, and an environment variable that sets it is `UnknownEnvVariable`. rust-milter has no release yet, so no published configuration breaks.
- The library no longer checks the operator's descriptor budget against `max_connections`. The process limit and the `EMFILE` back-off cover what it guarded against.

## References

- [configuration.md, ResourceLimitsConfig](../reference/configuration.md#resourcelimitsconfig)
- [Deploy a milter in production, Raise the file descriptor limit](../how-to/deploy-a-milter-in-production.md#raise-the-file-descriptor-limit)
- [tcp-server.md, Error recovery](../reference/tcp-server.md#error-recovery)
