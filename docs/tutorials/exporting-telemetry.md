# Export metrics with OpenTelemetry

Status: rust-milter is not implemented yet. This tutorial follows the designed API in [telemetry.md](../reference/telemetry.md).

In this tutorial you send the header tagger's metrics to an OpenTelemetry Collector and see them arrive.

## What you need

- the `header-tagger` project from [Getting started](getting-started.md)
- Docker, to run the Collector

## 1. Start a Collector

Create `otelcol.yaml`:

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

Start the Collector:

```sh
docker run --rm -p 4317:4317 \
  -v "$(pwd)/otelcol.yaml:/etc/otelcol/config.yaml" \
  otel/opentelemetry-collector:latest
```

Leave it running in its own terminal.

## 2. Turn on the `otel` feature and add the SDK

```sh
cargo add rust-milter --features otel
cargo add opentelemetry opentelemetry_sdk
cargo add opentelemetry-otlp --features grpc-tonic,metrics
```

`cargo add` picks the latest release of each crate. The three OpenTelemetry crates must come from the same release line, so add them together.

## 3. Install the meter provider

Change `main` in `src/main.rs`:

```rust
use opentelemetry_otlp::MetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let exporter = MetricExporter::builder().with_tonic().build()?;
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(Resource::builder().with_service_name("header-tagger").build())
        .build();

    let config = ServerConfig {
        bind_addresses: vec!["127.0.0.1:8891".parse()?],
        ..ServerConfig::default()
    };

    let server = Server::builder()
        .config(config)
        .meter_provider(meter_provider.clone())
        .handler(HeaderTagger::new)
        .build()?;

    server.run().await?;

    // Send the last measurements before the process exits.
    meter_provider.shutdown()?;
    Ok(())
}
```

The exporter sends to `localhost:4317` unless `OTEL_EXPORTER_OTLP_ENDPOINT` says otherwise.

## 4. Run the milter and send mail

```sh
cargo run
```

Send a few test messages through Postfix, as in [Getting started](getting-started.md).

## 5. Read the metrics

Within a minute, the Collector's terminal prints the metrics it received. Look for these metric names:
- `milter.server.connection.accepted`
- `milter.server.command`
- `milter.server.eom.duration`

`milter.server.command` has one data point per command type, each with the attribute `milter.command` set to a value such as `eom`.

To send the metrics on to Prometheus or another backend, add an exporter to the Collector configuration. The metric names and attributes are listed in [telemetry.md](../reference/telemetry.md#metrics).
