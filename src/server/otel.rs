//! The OpenTelemetry instruments, with the `otel` feature. The counters and
//! gauges are observable: they read the same counts as `status()` when the
//! SDK collects. See "Metrics" in `docs/reference/telemetry.md`.

use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use opentelemetry::metrics::{
    Histogram, Meter, MeterProvider, ObservableCounter, ObservableGauge, ObservableUpDownCounter,
};
use opentelemetry::{InstrumentationScope, KeyValue};

use super::ConfigWatch;
use super::status::Counters;

/// The bucket boundaries of every histogram, in seconds.
const BUCKETS: [f64; 15] = [
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0,
];

/// The histograms, recorded once per connection, message, or end of
/// message.
#[derive(Debug)]
pub(crate) struct Histograms {
    connection: Histogram<f64>,
    message: Histogram<f64>,
    eom: Histogram<f64>,
}

impl Histograms {
    pub(crate) fn connection(&self, duration: Duration, reason: &'static str) {
        self.connection.record(
            duration.as_secs_f64(),
            &[KeyValue::new("milter.close_reason", reason)],
        );
    }

    pub(crate) fn message(&self, duration: Duration, reply: &'static str) {
        self.message.record(
            duration.as_secs_f64(),
            &[KeyValue::new("milter.reply", reply)],
        );
    }

    pub(crate) fn eom(&self, duration: Duration, reply: &'static str) {
        self.eom.record(
            duration.as_secs_f64(),
            &[KeyValue::new("milter.reply", reply)],
        );
    }
}

/// Every instrument of one server. The observable ones are kept so that
/// their callbacks stay registered while the server exists.
pub(crate) struct Instruments {
    pub(crate) histograms: Arc<Histograms>,
    _counters: Vec<ObservableCounter<u64>>,
    _active: ObservableUpDownCounter<i64>,
    _generation: ObservableGauge<u64>,
    _last_reload: ObservableGauge<f64>,
}

/// A meter for rust-milter from `provider`, with the crate version as the
/// scope version.
pub(crate) fn meter(provider: &dyn MeterProvider) -> Meter {
    let scope = InstrumentationScope::builder("rust-milter")
        .with_version(env!("CARGO_PKG_VERSION"))
        .build();
    provider.meter_with_scope(scope)
}

/// What a counter observes: its values, each with its attributes.
type Observe = Box<dyn Fn(&Counters) -> Vec<(u64, Vec<KeyValue>)> + Send + Sync>;

/// The `server.address` and `server.port` attributes of a listener.
fn listener_attributes(address: std::net::SocketAddr) -> [KeyValue; 2] {
    [
        KeyValue::new("server.address", address.ip().to_string()),
        KeyValue::new("server.port", i64::from(address.port())),
    ]
}

/// Creates the instruments of telemetry.md on `meter`.
pub(crate) fn register<A: Send + Sync + 'static>(
    meter: &Meter,
    counters: &Arc<Counters>,
    watch: &ConfigWatch<A>,
) -> Instruments {
    let histogram = |name: &'static str, description: &'static str| {
        meter
            .f64_histogram(name)
            .with_unit("s")
            .with_description(description)
            .with_boundaries(BUCKETS.to_vec())
            .build()
    };
    let histograms = Arc::new(Histograms {
        connection: histogram(
            "milter.server.connection.duration",
            "Time from accept() to close",
        ),
        message: histogram(
            "milter.server.message.duration",
            "Time from SMFIC_MAIL to the final reply at end of message",
        ),
        eom: histogram(
            "milter.server.eom.duration",
            "Time from SMFIC_BODYEOB to the final reply",
        ),
    });

    let mut observed = Vec::new();
    let mut counter =
        |name: &'static str, unit: &'static str, description: &'static str, observe: Observe| {
            let counters = Arc::clone(counters);
            observed.push(
                meter
                    .u64_observable_counter(name)
                    .with_unit(unit)
                    .with_description(description)
                    .with_callback(move |observer| {
                        for (value, attributes) in observe(&counters) {
                            observer.observe(value, &attributes);
                        }
                    })
                    .build(),
            );
        };
    counter(
        "milter.server.connection.accepted",
        "{connection}",
        "Connections accepted, per listener",
        Box::new(|counters| {
            counters
                .snapshot(0)
                .listeners
                .iter()
                .map(|l| {
                    (
                        l.connections_accepted,
                        listener_attributes(l.address).to_vec(),
                    )
                })
                .collect()
        }),
    );
    counter(
        "milter.server.connection.rejected",
        "{connection}",
        "Connections closed at once because max_connections or max_memory was reached",
        Box::new(|counters| {
            counters
                .snapshot(0)
                .listeners
                .iter()
                .map(|l| {
                    (
                        l.connections_rejected,
                        listener_attributes(l.address).to_vec(),
                    )
                })
                .collect()
        }),
    );
    counter(
        "milter.server.connection.closed",
        "{connection}",
        "Connections closed, by reason",
        Box::new(|counters| {
            counters
                .snapshot(0)
                .connections_closed
                .into_iter()
                .map(|(reason, error, count)| {
                    let mut attributes = vec![KeyValue::new("milter.close_reason", reason)];
                    attributes.extend(error.map(|error| KeyValue::new("error.type", error)));
                    (count, attributes)
                })
                .collect()
        }),
    );
    counter(
        "milter.server.io",
        "By",
        "Bytes received and sent",
        Box::new(|counters| {
            let traffic = &counters.traffic;
            vec![
                (
                    traffic.bytes_received(),
                    vec![KeyValue::new("network.io.direction", "receive")],
                ),
                (
                    traffic.bytes_sent(),
                    vec![KeyValue::new("network.io.direction", "transmit")],
                ),
            ]
        }),
    );
    counter(
        "milter.server.negotiation",
        "{negotiation}",
        "Completed option negotiations, by negotiated version",
        Box::new(|counters| {
            counters
                .traffic
                .negotiations()
                .into_iter()
                .map(|(version, count)| {
                    let version = KeyValue::new("milter.protocol.version", i64::from(version));
                    (count, vec![version])
                })
                .collect()
        }),
    );
    counter(
        "milter.server.command",
        "{command}",
        "Commands received from the MTA",
        Box::new(|counters| {
            counters
                .traffic
                .commands()
                .into_iter()
                .map(|(command, count)| (count, vec![KeyValue::new("milter.command", command)]))
                .collect()
        }),
    );
    counter(
        "milter.server.reply",
        "{reply}",
        "Replies sent, by the command they answer",
        Box::new(|counters| {
            counters
                .traffic
                .replies()
                .into_iter()
                .map(|(command, reply, count)| {
                    let attributes = vec![
                        KeyValue::new("milter.command", command),
                        KeyValue::new("milter.reply", reply),
                    ];
                    (count, attributes)
                })
                .collect()
        }),
    );
    counter(
        "milter.config.reload",
        "{reload}",
        "Reload attempts",
        Box::new(|counters| {
            counters
                .snapshot(0)
                .reloads
                .into_iter()
                .map(|(result, source, count)| {
                    let attributes = vec![
                        KeyValue::new("milter.reload_result", result),
                        KeyValue::new("milter.reload_source", source),
                    ];
                    (count, attributes)
                })
                .collect()
        }),
    );

    let active = {
        let counters = Arc::clone(counters);
        meter
            .i64_observable_up_down_counter("milter.server.connection.active")
            .with_unit("{connection}")
            .with_description("Open connections, per listener")
            .with_callback(move |observer| {
                for listener in counters.snapshot(0).listeners {
                    let active = i64::try_from(listener.connections_active).unwrap_or(i64::MAX);
                    observer.observe(active, &listener_attributes(listener.address));
                }
            })
            .build()
    };
    let generation = {
        let watch = watch.clone();
        meter
            .u64_observable_gauge("milter.config.generation")
            .with_unit("{generation}")
            .with_description("Current configuration generation")
            .with_callback(move |observer| observer.observe(watch.generation(), &[]))
            .build()
    };
    let last_reload = {
        let counters = Arc::clone(counters);
        meter
            .f64_observable_gauge("milter.config.last_reload.time")
            .with_unit("s")
            .with_description("Unix time of the last applied reload")
            .with_callback(move |observer| {
                let last = counters.snapshot(0).last_reload;
                if let Some(time) = last.and_then(|t| t.duration_since(UNIX_EPOCH).ok()) {
                    observer.observe(time.as_secs_f64(), &[]);
                }
            })
            .build()
    };
    Instruments {
        histograms,
        _counters: observed,
        _active: active,
        _generation: generation,
        _last_reload: last_reload,
    }
}
