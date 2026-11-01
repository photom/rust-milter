# Test scenarios: Telemetry

Scenarios for metrics, traces, and the data rules. Unless a scenario says otherwise, the test builds the server with the `otel` feature, an in-memory exporter from the OpenTelemetry SDK's test utilities, and `tracing-opentelemetry`. Conventions are in [README.md](README.md).

## Feature and providers

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-001 | integration | The crate without the `otel` feature | CI lists its normal dependencies (`cargo tree -e normal`) | no `opentelemetry` crate appears | [telemetry: Requirements](../reference/telemetry.md#requirements) |
| TEL-002 | integration | The crate with the `otel` feature | CI lists its normal dependencies | `opentelemetry` appears, and `opentelemetry_sdk` and every exporter crate do not | [telemetry: Requirements](../reference/telemetry.md#requirements) |
| TEL-003 | integration | A server with the `otel` feature and no SDK installed | the fake MTA runs a complete session | the session succeeds, and `status()` counts the connection, commands, and replies | [telemetry: Requirements](../reference/telemetry.md#requirements) |
| TEL-004 | integration | A meter provider passed to `ServerBuilder::meter_provider`, and a different global provider | the fake MTA runs a session | the metrics reach only the builder's provider | [telemetry: Configuration and API](../reference/telemetry.md#configuration-and-api) |

## Metrics

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-010 | integration | A server and a fake MTA that runs sessions covering every command, every reply, a rejected connection, and a reload | metrics are collected | every metric in the tables of telemetry.md is reported, with its name, instrument type, unit, and attribute set, and no other metric is | [telemetry: Metrics](../reference/telemetry.md#metrics) |
| TEL-011 | integration | The same run | the collected values are compared with `status()` | every counter and gauge equals the matching `status()` field | [telemetry: Metrics](../reference/telemetry.md#metrics) |
| TEL-012 | integration | The same run | the histograms are read | each uses the bucket boundaries listed in telemetry.md, in seconds | [telemetry: Metrics](../reference/telemetry.md#metrics) |
| TEL-013 | integration | A connection closed by the fake MTA and one closed for a protocol error | `milter.server.connection.closed` is read | the first has `milter.close_reason` = `client_closed` and no `error.type`; the second has `error` and `error.type` = `Protocol` | [telemetry: Server metrics](../reference/telemetry.md#server-metrics) |
| TEL-014 | integration | `max_connections` = 1 and a second connection | metrics are collected | `milter.server.connection.rejected` is 1, and `milter.server.connection.closed` with `limit_reached` is 1 | [telemetry: Server metrics](../reference/telemetry.md#server-metrics) |
| TEL-015 | integration | A session with two messages, one with 50 body chunks | metrics are collected | `milter.server.message.duration` and `milter.server.eom.duration` each have a count of 2 | [telemetry: Performance](../reference/telemetry.md#performance) |
| TEL-016 | integration | One applied reload from `SIGHUP`, one failed reload from `reload()`, and a pending `backlog` change | the reload metrics are collected | `milter.config.reload` is 1 for `applied`/`sighup` and 1 for `failed`/`api`; `milter.config.generation` is 2; `milter.config.pending_restart` is 1; `milter.config.last_reload.time` is the time of the applied reload | [telemetry: Configuration reload metrics](../reference/telemetry.md#configuration-reload-metrics) |
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

## Data rules and logs

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| TEL-030 | integration | A session with marker values in the client hostname, client address, HELO name, sender, recipient, ESMTP arguments, header names and values, body, every macro, a `Reply` text, and a quarantine reason, at log level TRACE | every exported metric, span, span event, and log record is searched for the markers | none appears, except the queue ID on the message span | [telemetry: Data that must never be recorded](../reference/telemetry.md#data-that-must-never-be-recorded) |
| TEL-031 | integration | `tracing-opentelemetry` and `opentelemetry-appender-tracing` installed together | the server logs inside a session | each log record carries the trace ID and span ID of the span it was emitted in | [telemetry: Logs](../reference/telemetry.md#logs) |
