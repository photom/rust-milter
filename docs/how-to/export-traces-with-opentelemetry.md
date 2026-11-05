# Export traces with OpenTelemetry

This guide follows [telemetry.md](../reference/telemetry.md#traces). rust-milter is not on crates.io yet, so the guide takes it from its GitHub repository.

This guide shows how to send a rust-milter server's spans to an OpenTelemetry Collector over OTLP. rust-milter records its spans with `tracing`, so traces need no rust-milter feature: the application turns the spans into OpenTelemetry spans with `tracing-opentelemetry`.

## Add the crates

```sh
cargo add rust-milter --git https://github.com/photom/rust-milter
cargo add opentelemetry@0.33 opentelemetry_sdk@0.33
cargo add opentelemetry-otlp@0.33 --features grpc-tonic,trace
cargo add tracing-opentelemetry@0.34
cargo add tracing-subscriber --features fmt
```

`tracing-opentelemetry` 0.34 works with the 0.33 release line of the OpenTelemetry crates.

## Install a tracer provider and the tracing layer

Build a tracer provider with an OTLP exporter, and add its tracer to the `tracing` subscriber as a layer. Install the subscriber before the server runs, so the first connection is traced. Shut the provider down after the server stops, so the spans still waiting in the batch are sent:

<!-- doctest-setup
use rust_milter::{Handler, HandlerContext, Server, ServerConfig};

struct MyHandler;

impl MyHandler {
    fn new(_ctx: &HandlerContext<()>) -> Self {
        MyHandler
    }
}

impl Handler for MyHandler {
    type Session = ();
    type Message = ();
}

async fn serve(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
---
Ok(())
}
-->
```rust
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

let exporter = SpanExporter::builder().with_tonic().build()?;
let tracer_provider = SdkTracerProvider::builder()
    .with_batch_exporter(exporter)
    .with_resource(Resource::builder().with_service_name("my-milter").build())
    .build();

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(tracing_opentelemetry::layer().with_tracer(tracer_provider.tracer("my-milter")))
    .init();

let server = Server::builder()
    .config(config)
    .handler(MyHandler::new)
    .build()?;

server.run().await?;
tracer_provider.shutdown()?;
```

The spans are at INFO level and the command events at DEBUG. If you add a level filter to the subscriber, keep INFO for the target `rust_milter`, or no span is recorded.

## Choose which commands become events

Each command becomes a `milter.command` event on its span. By default only the envelope commands do. To record every header and body chunk as well, set `trace_commands` in the `[telemetry]` table of your configuration:

```toml
[telemetry]
trace_commands = "all"
```

Set it to `"none"` to record the spans without command events. The setting is described in [configuration.md](../reference/configuration.md), and the events in [telemetry.md](../reference/telemetry.md#command-events).

## Point the exporter at your Collector

The exporter sends to `localhost:4317` by default. To send elsewhere, set the standard variable before starting the milter:

```sh
OTEL_EXPORTER_OTLP_ENDPOINT=http://collector.internal:4317 ./my-milter
```

## Receive the traces in a Collector

A minimal Collector configuration that receives OTLP over gRPC and prints the spans:

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
    traces:
      receivers: [otlp]
      exporters: [debug]
```

Run it with the core Collector image:

```sh
docker run --rm -p 4317:4317 \
  -v "$(pwd)/otelcol.yaml:/etc/otelcol/config.yaml" \
  otel/opentelemetry-collector:latest
```

## Check that traces arrive

Send a message through the milter. The Collector prints one trace per MTA connection: a `milter.connection` span, a `milter.session` span below it, and a `milter.message` span below that. To find the trace of one message, search for its queue ID in the `milter.queue_id` attribute of the message span. All spans and attributes are listed in [telemetry.md](../reference/telemetry.md#traces).

To send rust-milter's logs over OpenTelemetry too, with the IDs of the span they were written in, see [Logs](../reference/telemetry.md#logs) in telemetry.md.
