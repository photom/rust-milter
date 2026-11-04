# Configuration snapshots and reload

Why rust-milter publishes its configuration as immutable snapshots, why a reload applies some settings at once and others only to new connections or after a restart, and why the configuration file rules are strict. The settings are listed in [configuration.md](../reference/configuration.md) and the reload procedure in [hot-reload.md](../reference/hot-reload.md).

## Why reload instead of restart

An operator who changes a timeout or a connection limit could restart the milter. A restart, though, drops every open MTA connection. Each MTA then applies its failure policy to the messages in flight: Sendmail follows the filter's `F=` flags, and Postfix 3.11 or later answers the SMTP client with a 421 by default. On a busy server, a restart therefore interrupts real mail. A reload changes the configuration while connections keep running.

## Why the library does not read files

rust-milter is a library inside someone else's program. That program may keep its settings in TOML, in YAML, in environment variables, or in a database, and it often has its own settings next to rust-milter's. If the library read a file itself, it would force one location and one format on every application. So the application owns loading: it registers a loader, and rust-milter calls it on `SIGHUP` and validates what comes back.

Containers are the one place where a default helps. Docker and Kubernetes pass configuration as environment variables, and every containerized milter would otherwise write the same mapping code. rust-milter therefore offers helpers that read `ServerConfig` from variables with a chosen prefix, but only when the application calls them. Unknown variables with that prefix are errors, for the same reason unknown file keys are: a misspelled name must not be silently ignored. A process's environment is fixed when it starts, so configuration from environment variables changes by restarting the process. In Kubernetes that is a rollout, which drains each pod before it stops.

## Why snapshots

Each valid configuration is published once, as an immutable value with a generation number. A reload never edits a published value; it publishes the next one.

No reader can see a configuration halfway through a change: a reader holds either the old value or the new one. Readers never wait for a writer. Every log line and metric can name the generation it ran under, so an operator can check whether a reload took effect.

A connection takes the current snapshot when it is accepted and keeps it until it closes. One milter connection lives for one SMTP session with Sendmail and Postfix, usually seconds. Within one SMTP session of a reload, all traffic runs on the new values, without any connection being cut short. A connection that changed its timeouts in the middle of a command would be harder to reason about and gain little.

## Why some settings need a restart

The reload classes follow from when each setting is used:
- Settings read for every decision, such as `max_connections` or the shutdown timings, take effect as soon as the new snapshot is published.
- Settings applied when a connection starts, such as timeouts and socket options, take effect for new connections.
- Settings applied when a listener is created, such as `backlog` and `reuse_port`, need a restart. A listener that is already running cannot take a new value.
- Settings fixed before anything runs need a restart too. The application builds the Tokio runtime once, with its thread count, before the server exists. The handler, the reload loader, and the meter provider are fixed when the server is built. Cargo features are fixed at compile time.

A reload that changes a setting that requires a restart fails, and applies none of its changes. The server could apply the rest and report the setting as waiting for a restart, and an earlier design did. The reload then counted as applied while the server ran something other than what the loader returned, and an operator who missed the warning would believe the file was in effect. When the whole reload fails instead, every applied reload leaves the server running exactly the loaded configuration. The error names the settings, so the operator knows to restart, or to set them back and reload. [ADR 0007](../adr/0007-a-reload-that-changes-a-restart-setting-fails.md) records the decision.

## Why a lower connection limit closes nothing

When a reload lowers `max_connections` below the number of open connections, the server could close connections until it meets the new limit. It does not. A milter connection that looks idle is usually waiting for the next command of an SMTP session in progress. Closing it would make the MTA apply its failure policy to that session, which interrupts mail the same way a restart would. Instead, open connections finish normally, and new connections are closed at once until the count falls below the limit. The window is short because SMTP sessions are short. The decision is recorded in [ADR 0005](../adr/0005-reload-closes-no-connection-and-rebinds-no-listener.md).

## Why a changed backlog needs a restart

The backlog is the kernel's queue of connections that are established but not yet accepted, and it is set once, when the listener starts listening. Applying a new value to an address that stays in use would need a second listener on the same port, opened with `SO_REUSEPORT`, before the old one closes. Connections still waiting in the old queue would be dropped, the behavior differs between Linux and the BSDs, and it depends on `reuse_port` being available. A backlog change is rare, so it takes a restart instead. [ADR 0005](../adr/0005-reload-closes-no-connection-and-rebinds-no-listener.md) records the options that were weighed, and [ADR 0007](../adr/0007-a-reload-that-changes-a-restart-setting-fails.md) why a reload that changes it fails.

## Why the reload binds before it publishes

A reload that moves the server to a new address binds the new listener first, publishes the new configuration, and only then stops the old listener. At no moment is the server without a listener. If the new bind fails, nothing has been published yet, so the running configuration stays as it was.

Requests that arrive during a reload are folded into one more reload after it, which then gets the latest configuration from the loader. Running reloads in parallel would mean two writers racing for the listener set.

## Why handlers read their settings per message

A handler that read its application configuration on every command could see one threshold at `SMFIC_MAIL` and another at end of message, if a reload happened in between. Reading once per message, or once per SMTP session, gives each message one consistent view.

## Why the file rules are strict

With the `serde` feature, `ServerConfig` reads straight from a file. Field names become the file format, so the format has to stay stable across versions, much as Protocol Buffers messages do. Two rules carry most of that weight.

Missing keys take their defaults. A file written for an older rust-milter keeps working when a newer version adds settings, which is what makes adding a setting a compatible change.

Unknown keys are errors. Protocol Buffers ignores unknown fields, and serde can too, but for a configuration file that is dangerous. A misspelled `max_conections = 500` would be ignored, and the server would run with the default limit while the operator believes the new one applies. An error names the key, and on reload it leaves the running configuration in place, so the mistake costs nothing but a log line.

The remaining rules follow from names being the contract: a renamed field keeps its old name as an alias, and removing a field is a major-version change.
