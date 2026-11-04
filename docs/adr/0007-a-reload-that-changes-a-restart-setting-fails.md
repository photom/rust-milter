# ADR 0007: A reload that changes a setting that requires a restart fails

- Status: Accepted
- Date: 2026-11-04
- Affects: configuration reload
- Supersedes: the `backlog` decision of [ADR 0005](0005-reload-closes-no-connection-and-rebinds-no-listener.md)

## Context

Four settings take effect only when a listener is created: `backlog`, `socket_options.reuse_addr`, `socket_options.reuse_port`, and `socket_options.ipv6_only`. ADR 0005 decided that a reload keeps their running value on an address that stays bound, applies every other change, and reports the setting as `pending_restart`. A listener the reload adds gets the new value.

Building that showed its cost. Each listener has to remember the configuration it was bound with. The reload report, `status()`, and a metric carry a list that lasts across reloads, because a later reload that changes nothing still leaves the setting waiting. The reload counts as applied while the server runs something other than what the loader returned, and an operator who misses the warning believes the file is in effect.

## Options considered

- (a) Keep ADR 0005: apply the rest, report the setting as `pending_restart`.
- (b) Fail the reload when one of the four settings changes and an address that stays bound would need the new value; apply it when every listener that uses it is new.
- (c) Fail the reload whenever one of the four settings changes, whatever happens to the addresses.

## Decision

Option (c).

- A reload whose new configuration changes `backlog`, `reuse_addr`, `reuse_port`, or `ipv6_only` fails with `ReloadError::RestartRequired`. The error names every one of these settings that changed.
- The reload applies nothing: no listener is bound or stopped, and the generation stays the same. The failure is logged like any other failed reload.
- The four settings form the reload class `restart`. The class `new_listeners` is removed.

## Consequences

Positive:
- After every applied reload, the server runs exactly the configuration the loader returned.
- Nothing waits for a restart, so the report, `status()`, and the metrics have no `pending_restart`.
- Each listener no longer keeps the configuration it was bound with.
- One rule for operators: a changed listener setting means a restart.

Negative:
- A file that changes one of the four settings blocks its other changes until the process restarts, or until the setting is reverted.
- A reload that only adds an address can no longer give it a new `backlog`. ADR 0005's workaround, removing an address in one reload and adding it back in the next, is gone too.

## References

- [configuration.md, Reload classes](../reference/configuration.md#reload-classes)
- [hot-reload.md, What each setting does on reload](../reference/hot-reload.md#what-each-setting-does-on-reload)
- [Configuration snapshots and reload](../explanation/configuration-and-reload.md#why-some-settings-need-a-restart)
