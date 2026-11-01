# ADR 0006: Connection timeouts are split per frame, and the idle default follows libmilter

- Status: Accepted
- Date: 2026-11-01
- Affects: server, configuration

## Context

The server configuration has four connection timeouts, `connect`, `read`, `write`, and `idle`, but the specification named them without saying what each one measures. `idle` defaulted to 300 seconds.

How long the MTA can leave a milter connection silent depends on the SMTP client. Between two SMTP commands, the MTA sends nothing to the milter:
- Sendmail waits up to 1 hour for a client's next SMTP command (`Timeout.command`, set in `sendmail/readcf.c`).
- Postfix waits up to 300 seconds (`smtpd_timeout`).
- libmilter waits 7210 seconds for any read or write (`MI_TIMEOUT`).

If the milter closes a connection while the SMTP session is still open, the MTA applies its failure policy to that session: Sendmail's `F=` flags, or Postfix's `milter_default_action`.

A single long timeout for every read, as in libmilter, also lets a peer that sends half a frame and then stops hold the connection for the whole timeout.

## Options considered

For the meaning of the timeouts:
- (a) One timeout for every read and write, as in libmilter.
- (b) A split by what the connection is waiting for: the next command, the rest of a frame, or the MTA reading a reply.

For the `idle` default:
- (a) Keep 300 seconds, which fits Postfix's defaults.
- (b) 3610 seconds, ten seconds above Sendmail's 1-hour default.
- (c) 7210 seconds, libmilter's `MI_TIMEOUT`.

## Decision

Option (b) for the meaning:
- `connect`: from `accept()` until the MTA's first command arrives.
- `idle`: while the connection waits for the first byte of the MTA's next command.
- `read`: from the first byte of a frame until the frame is complete.
- `write`: while one reply is being written.
- Time a handler spends in a callback counts against none of them. The MTA's own milter timeouts limit it.

Option (c) for the default: `idle` is 7210 seconds.

## Consequences

Positive:
- With default MTA settings, rust-milter never closes a connection that the MTA still considers open, and a milter moved from libmilter keeps the same idle behavior.
- A peer that stops halfway through a frame is closed after `read` (30 seconds by default), not after the idle timeout.
- TCP keepalive, on by default, still detects an MTA host that has gone away during a long idle period.

Negative:
- Connections from an MTA that leaks them stay open up to 2 hours. They count against `max_connections` until they close.
- An operator who wants a shorter idle limit with Sendmail must also lower Sendmail's `Timeout.command`.

## References

- [configuration.md, TimeoutConfig](../reference/configuration.md#timeoutconfig)
- [tcp-server.md, Connection lifecycle](../reference/tcp-server.md#connection-lifecycle)
- [milter-protocol.md, Timeouts](../reference/milter-protocol.md#timeouts)
- Sendmail 8.19.0.2 source: `sendmail/readcf.c` (`TimeOuts.to_nextcommand`), `libmilter/libmilter.h` (`MI_TIMEOUT`)
- Postfix 3.12 source: `src/global/mail_params.h` (`DEF_SMTPD_TMOUT`)
