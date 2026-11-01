# ADR 0003: rust-milter never records the SMTP client's address

- Status: Accepted
- Date: 2026-11-01
- Affects: telemetry, logging, handler contract

## Context

`SMFIC_CONNECT` carries the SMTP client's hostname and address, and `SMFIC_HELO` its HELO name. The telemetry specification lists all three as data rust-milter must never record in a metric, span, span event, or log field.

An operator who investigates spam or abuse would like to search traces by client address and see every session from one IP, with the milter's decisions and timings. The question was whether rust-milter should offer that as an opt-in setting.

Facts that bear on it:
- Privacy laws such as the GDPR usually treat an IP address as personal data. Telemetry often travels further than mail logs: to observability vendors, to dashboards with broad access, and under other retention rules.
- Hashing does not anonymize an address. There are few enough IPv4 addresses to hash them all and compare.
- A span attribute creates no time series, so cardinality does not limit this choice. Privacy does.
- The handler's `connect` callback already receives the address in `ConnectInfo`, and a handler may add attributes to the current span and write its own logs.
- The message span carries the queue ID (`{i}`). The MTA's own log line for that queue ID holds the client's address.

## Options considered

- (a) An opt-in setting, off by default, that adds `client.address` to the `milter.session` span.
- (b) Never record the address in the library. An application that needs it records it from its `connect` callback.
- (c) Record a hashed or truncated address by default.

## Decision

Option (b). rust-milter never records the SMTP client's hostname, address, or HELO name, and no setting turns this on. An application that needs the address records it in its own `connect` callback, as a log field or a span attribute, and takes responsibility for that choice.

Option (c) was rejected because a hash of an IP address can be reversed. Option (a) would put a personal-data switch into the library's configuration, where an operator could turn it on without the application's authors knowing.

## Consequences

Positive:
- The library's telemetry contains no SMTP client data in any configuration, so the forbidden-data test has no exceptions to cover.
- An application that has a legal basis for recording the address needs a few lines in `connect`; the how-to guide shows them.

Negative:
- Each application that wants the address writes that code itself, and nothing in rust-milter keeps the attribute name the same across applications. The how-to guide uses the OpenTelemetry convention `client.address` to steer them toward one name.

## References

- [telemetry.md, Data that must never be recorded](../reference/telemetry.md#data-that-must-never-be-recorded)
- [Telemetry principles, Why the mail itself stays out](../explanation/telemetry-principles.md#why-the-mail-itself-stays-out)
- [Record the SMTP client's address](../how-to/record-the-smtp-client-address.md)
