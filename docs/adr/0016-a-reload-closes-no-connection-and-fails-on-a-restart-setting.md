# ADR 0016: A reload closes no open connection, and fails when it changes a setting that requires a restart

- Status: Accepted
- Date: 2026-11-09
- Supersedes: [ADR 0005](0005-reload-closes-no-connection-and-rebinds-no-listener.md) and [ADR 0007](0007-a-reload-that-changes-a-restart-setting-fails.md), which together held this decision
- Affects: configuration reload, server

## Context

A reload can change settings whose new value does not fit the running server.

`max_connections` can drop below the number of open connections. A milter connection that looks idle is usually waiting for the next command of an SMTP session in progress. If the milter closes it, the MTA applies its failure policy to that session: Sendmail's `F=` flags, or Postfix's `milter_default_action`, which answers the SMTP client with a 421 by default since Postfix 3.11.

Four settings take effect only when a listener is created: `backlog`, `socket_options.reuse_addr`, `socket_options.reuse_port`, and `socket_options.ipv6_only`. The kernel sets the backlog once, when the listener starts listening, and the socket options are set before `bind()`. Giving a running address a new value needs a second listener on the same port, opened with `SO_REUSEPORT` before the old one closes. Connections still waiting in the old queue are then dropped, the behavior differs between Linux and the BSDs, and it works only where `reuse_port` is available.

A reload that applies some settings and keeps the running value of others has a cost of its own. Each listener has to remember the configuration it was bound with. The reload report, `status()`, and a metric carry a list of settings waiting for a restart, which lasts across reloads. The reload counts as applied while the server runs something other than what the loader returned, and an operator who misses the warning believes the file is in effect.

## Options considered

For a lowered `max_connections`:
- (a) Close open connections until the count meets the new limit.
- (b) Close no open connection. Close new connections at once until the count drops below the new limit.

For a changed listener setting:
- (a) Open a second listener with `SO_REUSEPORT`, then close the old one.
- (b) Close the listener and bind it again, leaving a moment without a listener.
- (c) Keep the running value on addresses that stay bound, apply every other change, and report the setting as waiting for a restart.
- (d) Fail the reload when an address that stays bound would need the new value, and apply it when every listener that uses it is new.
- (e) Fail the reload whenever one of the four settings changes, whatever happens to the addresses.

## Decision

Option (b) for `max_connections` and option (e) for the listener settings.

- A lowered `max_connections` closes no open connection. New connections are closed at once until the count falls below the new limit.
- A reload whose new configuration changes `backlog`, `reuse_addr`, `reuse_port`, or `ipv6_only` fails with `ReloadError::RestartRequired`. The error names every one of these settings that changed.
- Such a reload applies nothing: no listener is bound or stopped, and the generation stays the same. The failure is logged like any other failed reload.
- The four settings form the reload class `restart`.
- A reload never closes and rebinds a listener on an address that stays in the configuration.

## Consequences

Positive:
- A reload never interrupts an SMTP session in progress, and never leaves an address without a listener.
- After every applied reload, the server runs exactly the configuration the loader returned. Nothing waits for a restart, so the report, `status()`, and the metrics carry no such list, and a listener does not keep the configuration it was bound with.
- One rule for operators: a changed listener setting means a restart.
- The reload behaves the same on every platform, with or without `SO_REUSEPORT`.

Negative:
- After a lowered limit, the server stays above it until enough sessions end. With Sendmail and Postfix one milter connection lasts one SMTP session, usually seconds.
- A file that changes one of the four settings blocks its other changes until the process restarts, or until the setting is reverted.
- A reload that only adds an address cannot give it a new `backlog` or socket option.

## References

- [configuration.md, Reload classes](../reference/configuration.md#reload-classes)
- [hot-reload.md](../reference/hot-reload.md)
- [Configuration snapshots and reload](../explanation/configuration-and-reload.md)
