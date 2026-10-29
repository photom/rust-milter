---
name: logging-principles
description: Guidelines for structured logging, telemetry, data privacy, and log levels. Tailored for server-side MTA integrations and the milter library. Use to ensure consistent observability.
---

# Logging Principles & Security (Milter Context)

This skill provides mandatory guidelines for logging, telemetry, and data privacy, specifically adapted for building and maintaining mail filters (milters) and server-side SMTP integrations.

## 1. Data Privacy & Masking

- **PII Protection**: Never log plain-text passwords, phone numbers, or highly sensitive personal data. While envelope addresses and message IDs are often logged for tracking, contents of the email body should be treated as highly sensitive.
- **Masking Rules**: 
  - Mask values for keys ending in `password`.
  - Mask values for keys starting with `sec_`.
  - Use common logging utility methods or filters to replace sensitive data with masks (e.g., `***`).
- **Array Handling**: Avoid logging indeterminate or large arrays in a single line. If the count exceeds human-readable limits, omit it from the logs.

## 2. Modern Observability & Telemetry

- **Structured Logging**: Use JSON or other structured formats (e.g., key-value pairs) to facilitate integration with Cloud services and SIEM (Security Information and Event Management) platforms.
- **Audit & Security**: Ensure audit/access logs comply with security requirements, including tampering detection and long-term retention.
- **Unified Telemetry**: Integrate Logs, Metrics, and Traces as a unified telemetry strategy to achieve high observability and real-time system insights. Use OpenTelemetry (OTLP) as the vendor-neutral standard; OpenTracing and OpenCensus are archived. In Rust, instrument with the `tracing` crate (spans + structured fields) rather than the plain `log` facade, and export via `tracing-opentelemetry` when needed.

## 🖥️ Server-Side Logging Guidelines (Milter)

### Operational Context

Logs must empower operators. In a milter context, every log line must include context identifying the SMTP transaction to allow immediate impact assessment and message tracing.

**Required Context for Milters:**
Always include these on the same line if available:
- `Connection ID` / `Session ID` (from the MTA)
- `Client IP` / `Hostname`
- `Message-ID` (from the email header)
- `Queue ID` (if provided by the MTA macro)

**Metrics (KGI/KPI/KAI)**: Output logs related to defined business and activity indicators (e.g., messages processed, spam detected, virus detected, quarantine actions) to enable continuous improvement cycles.

### Log Levels & Timing

| Level | Timing / Stage | Required Context |
| :--- | :--- | :--- |
| **INFO** | Milter Start/End, Connection Accepted, Envelope Sender/Recipients processed, Message Accepted/Rejected, External API Calls | Connection ID, Client IP, Message-ID, Action taken (Accept, Reject, Discard), SMTP reply codes, External request status. |
| **WARN** | MTA macro resolution failures, External API timeouts (non-fatal), Malformed headers | Connection ID, Error details, Endpoint/Params for external calls, Specific malformed data (non-sensitive). |
| **ERROR** | Unexpected errors / caught panics (Rust has no exceptions), Database connection failures, Critical external service failures causing message tempfails (`4xx`) | Connection ID, Message-ID (if known), Error content including the `source()` chain, Backtrace (`std::backtrace::Backtrace`, on a separate line). |
| **DEBUG** | Detailed parsing of headers/body, raw bytes received, internal state transitions | Connection ID, Class/Method names, specific header keys being checked, return values of internal functions. |

## 💻 Client-Side Logging Guidelines

*(Included for completeness if building client admin interfaces for the milter)*

**Debugging & Support**
For on-premise or client applications, ensure logs provide necessary information for IT administrators and support teams.

| Level | Timing | Required Context |
| :--- | :--- | :--- |
| **DEBUG** | Public Method Entry/Exit | Resource ID, Class/Method names, Arguments, Return values. |
| **INFO** | App Start/End, Validation Success, Successful mutations, External Comms | Version, Args, Input values, Status codes, Request/Response bodies. |
| **WARN** | External 5xx errors (Permanent/Input errors), Config validation failure | Resource ID, Endpoint, Error line/item, Failed input values. |
| **ERROR** | Unexpected errors / panics, External 4xx errors (Temporary/System errors), App crash | Resource ID, Error content, Backtrace (on a separate line). |

## Related Skills

- For domain-specific context regarding SMTP and MTAs, see the **milter-protocol** skill.
- For Rust implementation specifics, refer to the **rust-master** skill.
- For ensuring third-party interactions are properly logged and masked, see **external-integration**.