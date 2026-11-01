# Telemetry principles

Why rust-milter reports metrics and traces through OpenTelemetry the way it does, and why it records so little about the mail itself. The metric names, span structure, and rules are in [telemetry.md](../reference/telemetry.md).

## Why OpenTelemetry

Operators of a milter want to see connection counts, latency at end of message, error rates, and how one message moved through the filter. Those are three kinds of signal: metrics, traces, and logs. OpenTelemetry covers all three with one vendor-neutral model, and its Collector can forward them to Prometheus, Jaeger, Loki, or a commercial service. Building on it spares rust-milter from choosing a backend for its users.

## Why the library uses only the API

OpenTelemetry splits into an API, which code calls to record measurements, and an SDK, which decides what happens to them: sampling, batching, exporting. OpenTelemetry's guidance for libraries is to depend on the API only. rust-milter follows it.

The application then owns every decision that depends on its environment: which exporter, which endpoint, which sampling rate, and which resource attributes such as `service.name`. Without an installed SDK, every instrument in rust-milter is a no-op, so an application that does not want telemetry pays almost nothing. The `otel` Cargo feature goes one step further and removes the dependency entirely when it is off.

## Why traces go through tracing

rust-milter already emits `tracing` spans and events for its logs, and much of the Rust ecosystem uses the same crate. The `tracing-opentelemetry` bridge turns those spans into OpenTelemetry spans in the application. Using it means the library needs no OpenTelemetry trace API of its own, and logs and traces stay consistent because they come from the same spans.

## Why attributes have fixed values

Every distinct combination of attribute values on a metric becomes its own time series in the backend. A metric labeled with the SMTP client's address would create a new series for every client, and on a public mail server that is an unbounded number. Storage and query costs grow with it, and some backends start dropping data.

So every metric attribute in rust-milter takes its value from a small fixed set, defined in the reference and enforced by enums in the code. Addresses, IDs, and free text never become metric attributes. The listener address is the one exception, and it is bounded by the configuration.

## Why the mail itself stays out

A milter sees the most sensitive parts of email: who writes to whom, subject lines, message bodies, and authenticated user names. Telemetry data usually travels further than mail logs, to observability vendors and dashboards with broad access. rust-milter therefore records none of the SMTP data it sees: no envelope addresses, headers, body content, macro values, or reply text. No setting turns this on, not even for the SMTP client's address; [ADR 0003](../adr/0003-never-record-the-smtp-client-address.md) records why.

Two details make this easy to get wrong. First, the peer address on a server span is the MTA's address, not the SMTP client's; the SMTP client's address arrives inside the protocol and counts as session data. Second, the queue ID is the one macro value that is recorded. It contains no personal data, and it is the key that connects a trace to the MTA's own log lines, which is what an operator needs when a specific message goes wrong.

The same isolation rule as for connection reuse applies to spans: a session span ends with its SMTP session, and a later session on the same connection never becomes its child. See [Connection reuse and session isolation](connection-reuse.md).

## Why counters read atomics

The server needs its own counters anyway, for `ServerHandle::status()` and the health check. Making the OpenTelemetry counters observable instruments that read those atomics at collection time means each event costs one atomic increment, with or without the `otel` feature, and the numbers in `status()` and in the backend always agree. Histograms cannot work this way, so they record directly, but only once per connection, message, or end of message.

## Why traces start at the milter

The Milter protocol has no field for trace context, and Sendmail and Postfix could not fill one anyway. Each connection from an MTA therefore starts a new trace. The queue ID on the message span is the bridge back to the MTA.
