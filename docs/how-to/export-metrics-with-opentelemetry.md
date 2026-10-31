# Export metrics with OpenTelemetry

Status: rust-milter is not implemented yet. This guide follows the designed API in [telemetry.md](../reference/telemetry.md).

This guide shows how to send a rust-milter server's metrics to an OpenTelemetry Collector over OTLP, and from there to any backend.

## Enable the otel feature and add the SDK

```sh
cargo add rust-milter --features otel
cargo add opentelemetry opentelemetry_sdk
cargo add opentelemetry-otlp --features grpc-tonic,metrics
```

Add the three OpenTelemetry crates together so they come from the same release line.

## Install a meter provider

Build a meter provider with an OTLP exporter and pass it to the server builder. Shut it down after the server stops, so the last measurements are sent:

```rust
use opentelemetry_otlp::MetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;

let exporter = MetricExporter::builder().with_tonic().build()?;
let meter_provider = SdkMeterProvider::builder()
    .with_periodic_exporter(exporter)
    .with_resource(Resource::builder().with_service_name("my-milter").build())
    .build();

let server = Server::builder()
    .config(config)
    .meter_provider(meter_provider.clone())
    .handler(MyHandler::new)
    .build()?;

server.run().await?;
meter_provider.shutdown()?;
```

## Point the exporter at your Collector

The exporter sends to `localhost:4317` by default. To send elsewhere, set the standard variable before starting the milter:

```sh
OTEL_EXPORTER_OTLP_ENDPOINT=http://collector.internal:4317 ./my-milter
```

## Receive the metrics in a Collector

A minimal Collector configuration that receives OTLP over gRPC and prints the metrics:

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

exporters:
  debug:
    verbosity: detailed

service:
  pipelines:
    metrics:
      receivers: [otlp]
      exporters: [debug]
```

Run it with the core Collector image:

```sh
docker run --rm -p 4317:4317 \
  -v "$(pwd)/otelcol.yaml:/etc/otelcol/config.yaml" \
  otel/opentelemetry-collector:latest
```

## Send the metrics to Prometheus

Add a Prometheus exporter to the Collector and let Prometheus scrape the Collector. rust-milter itself needs no change. In Prometheus, the names appear with underscores and unit suffixes, for example `milter_server_connection_duration_seconds`.

## Check that metrics arrive

Send some mail through the milter. Within a minute, the Collector receives metrics named `milter.server.connection.accepted`, `milter.server.command`, and `milter.server.eom.duration`. All names and attributes are listed in [telemetry.md](../reference/telemetry.md#metrics).

## Choose how much tracing detail to record

Command events on spans are controlled by `telemetry.trace_commands` in `ServerConfig`: `"none"`, `"envelope"` (default), or `"all"`. The setting can be changed by a reload. Use `"all"` only while debugging; it records one event per header and body chunk.
