//! TEL-004, alone in its binary because it sets the global meter provider.

#![cfg(feature = "otel")]

mod support;

use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use rust_milter::{Server, ServerConfig};
use support::mta::{Mta, Reach};
use support::ports::PortLease;
use support::{ContinueAll, Until};

fn provider() -> (SdkMeterProvider, InMemoryMetricExporter) {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    (provider, exporter)
}

/// The names of the metrics `exporter` received in its last collection.
fn names(provider: &SdkMeterProvider, exporter: &InMemoryMetricExporter) -> Vec<String> {
    exporter.reset();
    provider.force_flush().unwrap();
    let mut names = Vec::new();
    for resource in exporter.get_finished_metrics().unwrap() {
        for scope in resource.scope_metrics() {
            for metric in scope.metrics() {
                names.push(metric.name().to_owned());
            }
        }
    }
    names
}

#[tokio::test]
async fn tel_004_metrics_reach_only_the_builders_provider() {
    let (global, global_exporter) = provider();
    opentelemetry::global::set_meter_provider(global.clone());
    let (own, own_exporter) = provider();
    let lease = PortLease::acquire().await;
    let config = ServerConfig {
        bind_addresses: vec![lease.v4()],
        shutdown: support::quick_shutdown(),
        ..ServerConfig::default()
    };
    let server = Server::builder()
        .config(config)
        .meter_provider(own.clone())
        .handler(ContinueAll::new)
        .build()
        .unwrap();
    let handle = server.handle();
    let task = tokio::spawn(server.run());
    support::wait_until(&handle, Until::Accepting).await;
    let mut mta = Mta::connect(lease.v4()).await;
    mta.reach(Reach::Endm).await;
    mta.close().await;
    support::wait_until(&handle, Until::Idle).await;

    assert!(names(&own, &own_exporter).contains(&"milter.server.command".to_owned()));
    assert!(names(&global, &global_exporter).is_empty());
    handle.shutdown();
    task.await.unwrap().unwrap();
}
