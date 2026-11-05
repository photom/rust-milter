# Test scenarios: Telemetry

Scenarios for metrics, traces, and the data rules. The trace and log scenarios need no Cargo feature: the tests send the server's `tracing` spans and events to the in-memory exporters of the OpenTelemetry SDK's test utilities, through `tracing-opentelemetry` and `opentelemetry-appender-tracing`. Unless a scenario says otherwise, the metric scenarios build the server with the `otel` feature and an in-memory metric exporter. Conventions are in [README.md](README.md).

## Feature and providers

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-001 | integration | The crate without the `otel` feature | a test lists its normal dependencies with `cargo tree -e normal` | no `opentelemetry` crate appears | [telemetry: Requirements](../reference/telemetry.md#requirements) |
| TEL-002 | integration | The crate with the `otel` feature | a test lists its normal dependencies | `opentelemetry` appears, and `opentelemetry_sdk` and every exporter crate do not | [telemetry: Requirements](../reference/telemetry.md#requirements) |
| TEL-003 | integration | A server with the `otel` feature and no SDK installed | the fake MTA runs a complete session | the session succeeds, and `status()` counts the connection, commands, and replies | [telemetry: Requirements](../reference/telemetry.md#requirements) |
| TEL-004 | integration | A meter provider passed to `ServerBuilder::meter_provider`, and a different global provider | the fake MTA runs a session | the metrics reach only the builder's provider | [telemetry: Configuration and API](../reference/telemetry.md#configuration-and-api) |
| TEL-005 | integration | A server, with or without the `otel` feature, and one session with `MAIL`, `RCPT`, end of message, and `SMFIC_QUIT` | the session ends and `status()` is read | `commands` lists `optneg`, `connect`, `helo`, `mail`, `rcpt`, `eom`, and `quit` once each; `replies` has `optneg`/`optneg` and `continue` for the five commands that answer it; `negotiations` is `[(6, 1)]`; `connections_closed` is `[("client_closed", None, 1)]`; `bytes_received` and `bytes_sent` are more than 0; the listener's `connections_accepted` is 1 and `connections_active` 0 | [api: ServerHandle](../reference/api-specification.md#serverhandle) |
| TEL-006 | integration | A server on A that accepted one connection, and a reload that keeps A and adds B | `status()` is read | A's `connections_accepted` is still 1, and B's is 0 | [api: ServerHandle](../reference/api-specification.md#serverhandle) |

## Metrics

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-010 | integration | A server with `max_connections` = 1, and a fake MTA that runs a session through every stage, opens a second connection that is rejected, ends a third with a protocol error, and reloads | metrics are collected | every metric in the tables of telemetry.md is reported, with its name, instrument type, unit, and attribute set, and no other metric is | [telemetry: Metrics](../reference/telemetry.md#metrics) |
| TEL-011 | integration | The same run | the collected values are compared with `status()` | every counter and gauge equals the matching `status()` field | [telemetry: Metrics](../reference/telemetry.md#metrics) |
| TEL-012 | integration | The same run | the histograms are read | each uses the bucket boundaries listed in telemetry.md, in seconds | [telemetry: Metrics](../reference/telemetry.md#metrics) |
| TEL-013 | integration | A connection closed by the fake MTA and one closed for a protocol error | `milter.server.connection.closed` is read | the first has `milter.close_reason` = `client_closed` and no `error.type`; the second has `error` and `error.type` = `Protocol` | [telemetry: Server metrics](../reference/telemetry.md#server-metrics) |
| TEL-014 | integration | `max_connections` = 1 and a second connection | metrics are collected | `milter.server.connection.rejected` is 1, and `milter.server.connection.closed` with `limit_reached` is 1 | [telemetry: Server metrics](../reference/telemetry.md#server-metrics) |
| TEL-015 | integration | A session with two messages, one with 50 body chunks | metrics are collected | `milter.server.message.duration` and `milter.server.eom.duration` each have a count of 2 | [telemetry: Performance](../reference/telemetry.md#performance) |
| TEL-016 | integration | One applied reload from `SIGHUP`, and one reload from `reload()` that fails because it changes `backlog` | the reload metrics are collected | `milter.config.reload` is 1 for `applied`/`sighup` and 1 for `failed`/`api`; `milter.config.generation` is 2; `milter.config.last_reload.time` is the time of the applied reload | [telemetry: Configuration reload metrics](../reference/telemetry.md#configuration-reload-metrics) |
| TEL-017 | integration | Metrics after one connection, and again after 10,000 connections from different source ports | the number of time series per metric is counted | the counts are equal | [telemetry: Requirements](../reference/telemetry.md#requirements) |

## Traces

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-020 | integration | A fake MTA that runs two sessions on one connection, separated by `SMFIC_QUIT_NC`, with two messages in the first | the spans are exported | one `milter.connection` span (kind SERVER), two `milter.session` spans, and two `milter.message` spans under the first session; the second session has no parent from the first | [telemetry: Span structure](../reference/telemetry.md#span-structure) |
| TEL-021 | integration | A message whose end-of-message macros include `{i}` = `4XyZ12`, and one without `{i}` | the spans are exported | the first message span has `milter.queue_id` = `4XyZ12`; the second has no `milter.queue_id` | [telemetry: Span attributes](../reference/telemetry.md#span-attributes) |
| TEL-022 | integration | A connection over IPv4 | the spans are exported | the connection span has `network.transport` = `tcp`, `network.type` = `ipv4`, the MTA's `network.peer.address` and port, the listener's `server.address` and port, `milter.connection.id`, and `milter.protocol.version` | [telemetry: Span attributes](../reference/telemetry.md#span-attributes) |
| TEL-023 | integration | A connection that ends with a protocol error | the spans are exported | the connection span has status `Error` and `error.type` = `Protocol` | [telemetry: Span attributes](../reference/telemetry.md#span-attributes) |
| TEL-024 | integration | `trace_commands` = `none`, `envelope`, and `all`, in three runs of the same session with three headers and two body chunks | the span events are counted | `none` gives no `milter.command` events; `envelope` gives one per envelope command listed in telemetry.md; `all` adds one per header, body chunk, and macro | [telemetry: Command events](../reference/telemetry.md#command-events) |
| TEL-025 | integration | Every span from TEL-020 | the span names are read | each is one of `milter.connection`, `milter.session`, and `milter.message` | [telemetry: Span structure](../reference/telemetry.md#span-structure) |
| TEL-026 | integration | A handler whose `connect` adds an attribute to the current span | the spans are exported | the attribute is on the `milter.session` span | [telemetry: Data that must never be recorded](../reference/telemetry.md#data-that-must-never-be-recorded) |
| TEL-027 | integration | One session whose `connect` rejects it, and one with three messages: accepted at end of message, rejected in `rcpt` and then accepted at end of message, and aborted | the spans are exported | the first session span has `milter.reply` = `reject`; the second has none; the message spans have `accept`, `accept`, and none | [telemetry: Span attributes](../reference/telemetry.md#span-attributes) |
| TEL-028 | integration | `trace_commands` = `envelope`, and a session with `MAIL` and `SMFIC_ABORT` | the span events are read | the `mail` event is on the message span with `milter.reply` = `continue`; the `abort` event is on the same message span with `noreply`; the `connect` event is on the session span; each has a `milter.command.duration` of 0 or more seconds; the connection span has one `milter.negotiated` event with `milter.protocol.version` = 6 | [telemetry: Command events](../reference/telemetry.md#command-events) |
| TEL-029 | integration | A connection that closes after its idle timeout, and one that the fake MTA resets | the spans are exported | neither connection span has `error.type` or the status `Error` | [telemetry: Span attributes](../reference/telemetry.md#span-attributes) |

## Data rules and logs

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-030 | integration | A session with marker values in the client hostname, client address, HELO name, sender, recipient, ESMTP arguments, header names and values, body, every macro, a `Reply` text, and a quarantine reason, with `trace_commands` = `all` and no level filter | every exported span, span event, and log record is searched for the markers | none appears, except the queue ID on the message span | [telemetry: Data that must never be recorded](../reference/telemetry.md#data-that-must-never-be-recorded) |
| TEL-031 | integration | `tracing-opentelemetry` and `opentelemetry-appender-tracing` installed together | the server logs inside a session | each log record carries the trace ID and span ID of the span it was emitted in | [telemetry: Logs](../reference/telemetry.md#logs) |
| TEL-032 | integration | The session of TEL-030, with the `otel` feature and an in-memory metric exporter | every exported metric is searched for the markers | none appears | [telemetry: Data that must never be recorded](../reference/telemetry.md#data-that-must-never-be-recorded) |
