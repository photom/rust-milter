# ADR 0005: A reload closes no open connection and rebinds no listener

- Status: Accepted; the `backlog` decision is superseded by [ADR 0007](0007-a-reload-that-changes-a-restart-setting-fails.md)
- Date: 2026-11-01 (recorded; decided during the hot-reload design)
- Affects: configuration reload, server

## Context

A reload can change two settings whose new value does not fit the running server.

`max_connections` can drop below the number of open connections. A milter connection that looks idle is usually waiting for the next command of an SMTP session in progress. If the milter closes it, the MTA applies its failure policy to that session: Sendmail's `F=` flags, or Postfix's `milter_default_action`, which answers the SMTP client with a 421 by default since Postfix 3.11.

`backlog` can change on an address that stays bound. The kernel sets the backlog once, when the listener starts listening. Applying a new value needs a second listener on the same port, opened with `SO_REUSEPORT` before the old one closes. Connections still waiting in the old queue are then dropped, the behavior differs between Linux and the BSDs, and it works only where `reuse_port` is available.

## Options considered

For a lowered `max_connections`:
- (a) Close open connections until the count meets the new limit.
- (b) Close no open connection. Close new connections at once until the count drops below the new limit.

For a changed `backlog` on an address that stays bound:
- (a) Open a second listener with `SO_REUSEPORT`, then close the old one.
- (b) Close the listener and bind it again, leaving a moment without a listener.
- (c) Keep the running value and report the setting as needing a restart.

## Decision

Option (b) for `max_connections` and option (c) for `backlog`.

- A lowered `max_connections` closes no open connection. New connections are closed at once until the count falls below the new limit.
- A changed `backlog` on an address that stays bound keeps its running value, and the reload reports it as `pending_restart`. Removing the address in one reload and adding it back in the next applies the change without a restart.

## Consequences

Positive:
- A reload never interrupts an SMTP session in progress, and never leaves an address without a listener.
- The reload behaves the same on every platform, with or without `SO_REUSEPORT`.

Negative:
- After a lowered limit, the server stays above it until enough sessions end. With Sendmail and Postfix one milter connection lasts one SMTP session, usually seconds.
- A backlog change on a kept address needs a restart, or two reloads.

## References

- [configuration.md, Reload classes](../reference/configuration.md#reload-classes)
- [hot-reload.md](../reference/hot-reload.md)
- [Configuration snapshots and reload](../explanation/configuration-and-reload.md#why-a-lower-connection-limit-closes-nothing)
