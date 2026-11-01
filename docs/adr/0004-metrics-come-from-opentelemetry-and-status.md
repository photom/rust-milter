# ADR 0004: Metrics come from OpenTelemetry and status(), with no collector trait

- Status: Accepted
- Date: 2026-11-01
- Affects: telemetry, public API

## Context

Before the OpenTelemetry design existed, the library architecture listed a possible "metrics collector trait": a trait the application implements, with one method per event, such as a connection accepted or a reply sent. rust-milter would call it on every event, and the application would forward the numbers to any backend.

The telemetry design now gives two ways to get metrics:
- OpenTelemetry instruments behind the `otel` feature. The OpenTelemetry Collector forwards them to Prometheus and most other backends.
- `ServerHandle::status()`, which returns the same status counters in-process, with no extra dependency.

The status counters are atomics. Recording an event costs one atomic increment, with or without the `otel` feature.

Some applications avoid the OpenTelemetry crates, which are still at 0.x and often release breaking versions. Others use the `metrics` crate facade instead of OpenTelemetry.

## Options considered

- (a) Add the collector trait as a third mechanism.
- (b) Drop the trait. Metrics come from the OpenTelemetry instruments and `status()`.
- (c) Support the `metrics` crate facade now, as another optional feature.

## Decision

Option (b). rust-milter has no metrics collector trait, and none is planned.

- The trait would duplicate what the instruments and `status()` already provide.
- It would add a public trait that must stay stable after 1.0.
- It would cost a dynamic call per event, where an event now costs one atomic increment.
- Its method arguments would pass session data to application code, so the rule that telemetry carries no SMTP data would need enforcing in another place.

An application without OpenTelemetry reads `status()`. Option (c) stays possible: support for the `metrics` crate, or another facade, can come later as an optional Cargo feature. That change is additive and fits a minor release, so it needs no decision now.

## Consequences

Positive:
- One set of metric names, defined in telemetry.md, and one code path that records them.
- The hot path stays at one atomic increment per event.

Negative:
- An application that wants metrics without OpenTelemetry polls `status()` and converts the snapshot itself.

## References

- [telemetry.md](../reference/telemetry.md)
- [Telemetry principles, Why counters read atomics](../explanation/telemetry-principles.md#why-counters-read-atomics)
- [Library architecture, Extension points](../explanation/architecture.md#extension-points)
