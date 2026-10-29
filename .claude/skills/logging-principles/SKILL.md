---
name: logging-principles
description: Rules for structured logging, telemetry, data privacy, and log levels in the milter library and other server-side MTA integrations. Use when adding or reviewing log output, metrics, or tracing.
---

# Logging principles for milters

These rules are mandatory for mail filters (milters) and other server-side SMTP integrations.

## 1. Privacy and masking

- Never log plain-text passwords, phone numbers, or other highly sensitive personal data. Envelope addresses and message IDs are usually logged for tracing, but treat the message body as highly sensitive.
- Mask the values of keys that end in `password` or start with `sec_`.
- Do the masking in shared logging helpers or filters that replace the value with a mask such as `***`.
- Do not log large arrays, or arrays of unknown size, on one line. If an array is too long for a person to read, leave it out of the log.

## 2. Observability and telemetry

- Write structured logs, as JSON or key-value pairs, so cloud services and SIEM (Security Information and Event Management) platforms can ingest them.
- Audit and access logs must meet the security requirements, including tamper detection and long-term retention.
- Treat logs, metrics, and traces as one telemetry system. Use OpenTelemetry (OTLP) as the vendor-neutral standard; OpenTracing and OpenCensus are archived. In Rust, instrument code with the `tracing` crate (spans and structured fields) in place of the plain `log` facade, and export through `tracing-opentelemetry` when you need OpenTelemetry output.

## Server-side logging (milter)

### Transaction context

Operators read these logs to judge impact and trace messages, so every log line must identify its SMTP transaction. Put these fields on the same line whenever they are available:

- `Connection ID` / `Session ID` (from the MTA)
- `Client IP` / `Hostname`
- `Message-ID` (from the message header)
- `Queue ID` (when the MTA provides it as a macro)

Also log the business and activity indicators the project defines (KGI, KPI, KAI), such as messages processed, spam detected, viruses detected, and quarantine actions. The team uses these to measure and improve the filter.

### Log levels

| Level | When | Required context |
| :--- | :--- | :--- |
| INFO | Milter start and stop, connection accepted, envelope sender and recipients processed, message accepted or rejected, external API calls | Connection ID, client IP, Message-ID, action taken (accept, reject, discard), SMTP reply code, external request status |
| WARN | MTA macro resolution failures, non-fatal external API timeouts, malformed headers | Connection ID, error details, endpoint and parameters of external calls, the malformed data if it is not sensitive |
| ERROR | Unexpected errors and caught panics (Rust has no exceptions), database connection failures, external service failures that make the milter tempfail the message (`4xx`) | Connection ID, Message-ID if known, the error including its `source()` chain, the backtrace (`std::backtrace::Backtrace`) on a separate line |
| DEBUG | Detailed header and body parsing, raw bytes received, internal state transitions | Connection ID, type and function names, the header keys being checked, return values of internal functions |

## Client-side logging

This section applies only if the project builds a client or admin interface for the milter. For on-premises and client applications, the logs must give IT administrators and support staff what they need to debug.

| Level | When | Required context |
| :--- | :--- | :--- |
| DEBUG | Entry to and exit from public methods | Resource ID, type and method names, arguments, return values |
| INFO | App start and stop, validation success, successful changes, external communication | Version, arguments, input values, status codes, request and response bodies |
| WARN | External 5xx errors (permanent or input errors), configuration validation failures | Resource ID, endpoint, the failing line or item, the input values that failed |
| ERROR | Unexpected errors and panics, external 4xx errors (temporary or system errors), app crashes | Resource ID, the error, the backtrace on a separate line |

## Related skills

- milter-protocol: SMTP and MTA domain context.
- rust-master: Rust implementation details.
- external-integration: logging and masking for calls to third-party systems.
