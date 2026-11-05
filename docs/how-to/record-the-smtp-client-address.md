# Record the SMTP client's address

This guide follows [api-specification.md](../reference/api-specification.md) and [telemetry.md](../reference/telemetry.md).

rust-milter never records the SMTP client's address in its own logs, metrics, or spans. This guide shows how your handler records it when your application needs it, for example to investigate abuse.

## Decide whether you may record it

Privacy laws such as the GDPR usually treat an IP address as personal data. Before you record it, decide who can read your logs and traces, and how long they are kept. Hashing the address does not anonymize it: there are few enough IPv4 addresses to hash them all and compare.

## Log the address

The library passes the address to your `connect` callback in `ConnectInfo`. Log it there:

```rust
use rust_milter::{ConnectDecision, ConnectInfo, Handler, Result, SessionContext};

struct AddressLogger;

impl Handler for AddressLogger {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        if let Some(addr) = info.address {
            tracing::info!(client.address = %addr.ip(), "SMTP client connected");
        }
        Ok(ConnectDecision::Continue)
    }
}
```

`info.address` is `None` when the MTA does not know the client's address.

## Add the address to the session span

If your application exports traces with `tracing-opentelemetry`, add the address to the current span. rust-milter runs the `connect` callback inside the `milter.session` span, so the attribute lands on the session:

<!-- doctest-setup
fn record(info: &rust_milter::ConnectInfo) {
---
}
-->
```rust
use tracing_opentelemetry::OpenTelemetrySpanExt;

if let Some(addr) = info.address {
    tracing::Span::current().set_attribute("client.address", addr.ip().to_string());
}
```

`client.address` is the OpenTelemetry semantic convention name for the client's address.

## Keep the address out of metrics

Do not put the address on a metric attribute. Every distinct address would become its own time series in your backend. Why this matters is explained in [Telemetry principles](../explanation/telemetry-principles.md#why-attributes-have-fixed-values).
