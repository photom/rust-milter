# Deploy a milter in production

This guide follows [configuration.md](../reference/configuration.md) and [tcp-server.md](../reference/tcp-server.md).

This guide shows how to run a rust-milter milter safely next to a production MTA.

## Run it as an unprivileged user

Run the milter under its own system user, never as root. rust-milter needs no root privileges. Listen on a port above 1023, so no privilege is needed to bind it.

## Restrict who can connect

The milter protocol has no authentication and no encryption: anyone who can reach the port can feed it mail and read its replies.

1. If the MTA runs on the same host, bind only to loopback, for example `bind_addresses = ["127.0.0.1:8891"]`.
2. If the MTA runs on another host, bind to the internal interface only, and allow only the MTA's address in the firewall.
3. In a container, bind to all of the container's interfaces and restrict access outside it; see [Run a milter in Docker or Kubernetes](run-a-milter-in-docker-or-kubernetes.md).

## Raise the file descriptor limit

Each MTA connection uses one file descriptor, and each listener one more.

1. Set the process limit (`ulimit -n`, or `LimitNOFILE=` in a systemd unit) to at least `max_connections` plus the number of listeners plus 10; for the default `max_connections` of 10,000 and one listener, use at least 10,011.
2. If you set `resource_limits.max_fds`, keep it at least `max_connections` plus the listeners plus 10. The configuration is rejected otherwise.

When descriptors run out anyway, the server backs off and keeps running; watch for "Accept error" warnings in the log.

## Match the timeouts to the MTA

1. Keep the milter's `timeouts.idle` (7210 seconds by default) longer than the longest pause the MTA allows between two SMTP commands: Sendmail's `Timeout.command` (1 hour by default) or Postfix's `smtpd_timeout` (300 seconds by default).
2. Make sure end-of-message processing finishes within Sendmail's `E` timeout (5 minutes by default). Postfix's `milter_content_timeout` (300 seconds by default) applies to each reply, and the handler's `progress()` calls restart it.

## Decide what the MTA does when the milter fails

Choose the MTA's behavior for a milter that is down or slow:
- Postfix: set `milter_default_action` to `accept`, `reject`, `tempfail`, `quarantine`, or `shutdown` (the default since Postfix 3.11).
- Sendmail: set the filter's `F=` flags: `R` to reject, `T` to tempfail, `4` to shut the session down with 421, or none to accept as if the filter were not there.

The same policy applies when the milter closes new connections because `max_connections` is reached.

## Monitor it

1. Export metrics as described in [Export metrics with OpenTelemetry](export-metrics-with-opentelemetry.md).
2. Alert on `milter.server.connection.rejected` (the connection limit is reached) and on a rising `milter.server.eom.duration` (the milter is close to the MTA's end-of-message timeout).
3. Watch `ServerHandle::status()` or the health check from your process supervisor.

## Change settings without a restart

Reload the configuration with `SIGHUP` instead of restarting, so open SMTP sessions are not interrupted. See [Reload the configuration without a restart](reload-the-configuration.md).
