//! The metric and status-counter scenarios of `docs/testing/telemetry.md`.
//! TEL-004 changes the global meter provider, so it has its own binary,
//! `metrics_global.rs`.

mod support;

use std::process::Command;

use rust_milter::protocol::*;
use support::TestServer;
use support::mta::Reach;
use support::{ContinueAll, Until};

/// The crate's normal dependencies, as `cargo tree` lists them, with or
/// without the `otel` feature.
fn normal_dependencies(otel: bool) -> Vec<String> {
    let cargo = match std::env::var("CARGO") {
        Ok(cargo) => cargo,
        Err(_) => "cargo".to_owned(),
    };
    let mut command = Command::new(cargo);
    command.args([
        "tree",
        "--offline",
        "--edges",
        "normal",
        "--prefix",
        "none",
        "--format",
        "{p}",
        "--package",
        "rust-milter",
        "--no-default-features",
    ]);
    if otel {
        command.args(["--features", "rust-milter/otel"]);
    }
    let output = command.output().expect("cargo tree runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut crates = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(name) = line.split_whitespace().next() {
            crates.push(name.to_owned());
        }
    }
    crates
}

#[test]
fn tel_001_without_the_feature_no_opentelemetry_crate_is_linked() {
    let crates = normal_dependencies(false);

    for name in &crates {
        assert!(!name.starts_with("opentelemetry"), "{crates:?}");
    }
}

#[test]
fn tel_002_with_the_feature_only_the_api_crate_is_linked() {
    let crates = normal_dependencies(true);

    assert!(crates.contains(&"opentelemetry".to_owned()), "{crates:?}");
    for name in &crates {
        assert!(
            name == "opentelemetry" || !name.starts_with("opentelemetry"),
            "{name} in {crates:?}"
        );
    }
}

#[tokio::test]
async fn tel_005_status_counts_a_session() {
    let server = TestServer::start(ContinueAll).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;
    mta.ask_continue(SMFIC_BODYEOB, rust_milter::Bytes::new())
        .await;
    mta.send(SMFIC_QUIT, rust_milter::Bytes::new()).await;
    assert!(mta.closes().await);
    support::wait_until(&server.handle, Until::Idle).await;

    let status = server.handle.status();

    assert_eq!(
        status.commands,
        [
            ("optneg", 1),
            ("connect", 1),
            ("helo", 1),
            ("mail", 1),
            ("rcpt", 1),
            ("eom", 1),
            ("quit", 1),
        ]
    );
    assert_eq!(
        status.replies,
        [
            ("optneg", "optneg", 1),
            ("connect", "continue", 1),
            ("helo", "continue", 1),
            ("mail", "continue", 1),
            ("rcpt", "continue", 1),
            ("eom", "continue", 1),
        ]
    );
    assert_eq!(status.negotiations, [(6, 1)]);
    assert_eq!(status.connections_closed, [("client_closed", None, 1)]);
    assert!(
        status.bytes_received > 0 && status.bytes_sent > 0,
        "{status:?}"
    );
    let listener = &status.listeners[0];
    assert_eq!(listener.connections_accepted, 1);
    assert_eq!(listener.connections_active, 0);
    assert_eq!(listener.connections_rejected, 0);
    server.stop().await;
}

#[tokio::test]
async fn tel_006_a_kept_listener_keeps_its_counts() {
    let (a, b) = (
        support::ports::PortLease::acquire().await,
        support::ports::PortLease::acquire().await,
    );
    let config = rust_milter::ServerConfig {
        bind_addresses: vec![a.v4()],
        shutdown: support::quick_shutdown(),
        ..rust_milter::ServerConfig::default()
    };
    let server = rust_milter::Server::builder()
        .config(config.clone())
        .handler(ContinueAll::new)
        .build()
        .unwrap();
    let handle = server.handle();
    let task = tokio::spawn(server.run());
    support::wait_until(&handle, Until::Accepting).await;
    let mut mta = support::mta::Mta::connect(a.v4()).await;
    mta.negotiate().await;
    mta.close().await;
    support::wait_until(&handle, Until::Idle).await;

    let moved = rust_milter::ServerConfig {
        bind_addresses: vec![a.v4(), b.v4()],
        ..config
    };
    handle
        .reload_with_config(rust_milter::Reloaded {
            server: moved,
            app: (),
        })
        .await
        .unwrap();

    let mut accepted = Vec::new();
    for listener in handle.status().listeners {
        accepted.push((listener.address, listener.connections_accepted));
    }
    assert_eq!(accepted, [(a.v4(), 1), (b.v4(), 0)]);
    handle.shutdown();
    task.await.unwrap().unwrap();
}

#[cfg(feature = "otel")]
mod otel {
    use std::collections::{BTreeMap, BTreeSet};
    use std::io;
    use std::time::{SystemTime, UNIX_EPOCH};

    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
    use rust_milter::{
        Bytes, Decision, EomContext, Handler, MessageContext, MilterText, Reloaded, Result, Server,
        ServerConfig, ServerHandle,
    };
    use support::mta::{Mta, connect_data, strings};
    use support::ports::PortLease;

    use super::*;

    /// One data point, with what the tests compare.
    #[derive(Debug, Clone)]
    struct Point {
        name: String,
        kind: &'static str,
        unit: String,
        attributes: BTreeMap<String, String>,
        value: f64,
        count: u64,
        bounds: Vec<f64>,
    }

    /// A meter provider whose metrics the test reads on demand.
    struct Metrics {
        provider: SdkMeterProvider,
        exporter: InMemoryMetricExporter,
    }

    impl Metrics {
        fn new() -> Metrics {
            let exporter = InMemoryMetricExporter::default();
            let provider = SdkMeterProvider::builder()
                .with_reader(PeriodicReader::builder(exporter.clone()).build())
                .build();
            Metrics { provider, exporter }
        }

        /// Collects now, and returns every data point of the collection.
        fn collect(&self) -> Vec<Point> {
            self.exporter.reset();
            self.provider
                .force_flush()
                .expect("the metrics are collected");
            let exported = self
                .exporter
                .get_finished_metrics()
                .expect("the exporter can be read");
            let mut points = Vec::new();
            for resource in exported.iter().rev().take(1) {
                for scope in resource.scope_metrics() {
                    assert_eq!(scope.scope().name(), "rust-milter");
                    assert_eq!(scope.scope().version(), Some(env!("CARGO_PKG_VERSION")));
                    for metric in scope.metrics() {
                        points.extend(points_of(metric));
                    }
                }
            }
            points
        }
    }

    fn attributes<'a>(
        values: impl Iterator<Item = &'a opentelemetry::KeyValue>,
    ) -> BTreeMap<String, String> {
        let mut attributes = BTreeMap::new();
        for kv in values {
            attributes.insert(kv.key.to_string(), kv.value.to_string());
        }
        attributes
    }

    /// A point of `metric` without a count or bounds.
    fn point(
        metric: &opentelemetry_sdk::metrics::data::Metric,
        kind: &'static str,
        attributes: BTreeMap<String, String>,
        value: f64,
    ) -> Point {
        Point {
            name: metric.name().to_owned(),
            kind,
            unit: metric.unit().to_owned(),
            attributes,
            value,
            count: 0,
            bounds: Vec::new(),
        }
    }

    fn points_of(metric: &opentelemetry_sdk::metrics::data::Metric) -> Vec<Point> {
        let mut points = Vec::new();
        macro_rules! data {
            ($data:expr) => {
                match $data {
                    MetricData::Sum(sum) => {
                        let kind = if sum.is_monotonic() {
                            "counter"
                        } else {
                            "updowncounter"
                        };
                        for p in sum.data_points() {
                            let attributes = attributes(p.attributes());
                            points.push(point(metric, kind, attributes, p.value() as f64));
                        }
                    }
                    MetricData::Gauge(gauge) => {
                        for p in gauge.data_points() {
                            let attributes = attributes(p.attributes());
                            points.push(point(metric, "gauge", attributes, p.value() as f64));
                        }
                    }
                    MetricData::Histogram(histogram) => {
                        for p in histogram.data_points() {
                            let attributes = attributes(p.attributes());
                            let mut point = point(metric, "histogram", attributes, p.sum() as f64);
                            point.count = p.count();
                            point.bounds = p.bounds().collect();
                            points.push(point);
                        }
                    }
                    MetricData::ExponentialHistogram(_) => panic!("no exponential histogram"),
                }
            };
        }
        match metric.data() {
            AggregatedMetrics::F64(data) => data!(data),
            AggregatedMetrics::U64(data) => data!(data),
            AggregatedMetrics::I64(data) => data!(data),
        }
        points
    }

    fn named<'a>(points: &'a [Point], name: &str) -> Vec<&'a Point> {
        let mut named = Vec::new();
        for point in points {
            if point.name == name {
                named.push(point);
            }
        }
        named
    }

    fn value(points: &[Point], name: &str, attributes: &[(&str, &str)]) -> Option<f64> {
        let mut wanted = BTreeMap::new();
        for (key, value) in attributes {
            wanted.insert((*key).to_owned(), (*value).to_owned());
        }
        for point in named(points, name) {
            if point.attributes == wanted {
                return Some(point.value);
            }
        }
        None
    }

    /// A server on a leased port that reports to `metrics`.
    struct Running {
        handle: ServerHandle,
        task: tokio::task::JoinHandle<Result<()>>,
        lease: PortLease,
    }

    /// Starts a server with `handler` and `config` on a leased port, which
    /// replaces the bind addresses of `config`.
    async fn start<H: Handler>(metrics: &Metrics, mut config: ServerConfig, handler: H) -> Running {
        let lease = PortLease::acquire().await;
        config.bind_addresses = vec![lease.v4()];
        let server = Server::builder()
            .config(config)
            .meter_provider(metrics.provider.clone())
            .handler(move |_| handler)
            .build()
            .expect("a valid configuration");
        let handle = server.handle();
        let task = tokio::spawn(server.run());
        support::wait_until(&handle, Until::Accepting).await;
        Running {
            handle,
            task,
            lease,
        }
    }

    impl Running {
        async fn mta(&self) -> Mta {
            Mta::connect(self.lease.v4()).await
        }

        async fn idle(&self) {
            support::wait_until(&self.handle, Until::Idle).await;
        }

        async fn stop(self) {
            self.handle.shutdown();
            self.task.await.unwrap().unwrap();
        }
    }

    /// Every metric of telemetry.md: name, instrument, unit, and the
    /// attribute keys its points carry.
    const METRICS: [(&str, &str, &str, &[&str]); 14] = [
        (
            "milter.server.connection.accepted",
            "counter",
            "{connection}",
            &["server.address", "server.port"],
        ),
        (
            "milter.server.connection.active",
            "updowncounter",
            "{connection}",
            &["server.address", "server.port"],
        ),
        (
            "milter.server.connection.rejected",
            "counter",
            "{connection}",
            &["server.address", "server.port"],
        ),
        (
            "milter.server.connection.closed",
            "counter",
            "{connection}",
            &["milter.close_reason"],
        ),
        (
            "milter.server.connection.duration",
            "histogram",
            "s",
            &["milter.close_reason"],
        ),
        (
            "milter.server.io",
            "counter",
            "By",
            &["network.io.direction"],
        ),
        (
            "milter.server.negotiation",
            "counter",
            "{negotiation}",
            &["milter.protocol.version"],
        ),
        (
            "milter.server.command",
            "counter",
            "{command}",
            &["milter.command"],
        ),
        (
            "milter.server.reply",
            "counter",
            "{reply}",
            &["milter.command", "milter.reply"],
        ),
        (
            "milter.server.message.duration",
            "histogram",
            "s",
            &["milter.reply"],
        ),
        (
            "milter.server.eom.duration",
            "histogram",
            "s",
            &["milter.reply"],
        ),
        ("milter.config.generation", "gauge", "{generation}", &[]),
        (
            "milter.config.reload",
            "counter",
            "{reload}",
            &["milter.reload_result", "milter.reload_source"],
        ),
        ("milter.config.last_reload.time", "gauge", "s", &[]),
    ];

    /// The configuration of a test with `max_connections` = 1.
    fn one_connection() -> ServerConfig {
        let mut config = support::config();
        config.max_connections = 1;
        config
    }

    /// The run of TEL-010 to TEL-012: a whole session, a rejected
    /// connection, a protocol error, and a reload.
    async fn busy_run(metrics: &Metrics) -> Running {
        let server = start(metrics, one_connection(), ContinueAll).await;
        let mut mta = server.mta().await;
        mta.reach(Reach::Body).await;
        let mut rejected = server.mta().await;
        assert!(
            rejected.closes().await,
            "the second connection passes max_connections"
        );
        mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
        mta.send(SMFIC_QUIT, Bytes::new()).await;
        assert!(mta.closes().await);
        server.idle().await;
        let mut broken = server.mta().await;
        broken.negotiate().await;
        broken.send(b'Z', Bytes::new()).await;
        assert!(broken.closes().await);
        server.idle().await;
        let config = (*server.handle.config()).clone();
        server
            .handle
            .reload_with_config(Reloaded {
                server: config,
                app: (),
            })
            .await
            .unwrap();
        server
    }

    #[tokio::test]
    async fn tel_010_every_metric_is_reported_as_specified() {
        let metrics = Metrics::new();
        let server = busy_run(&metrics).await;

        let points = metrics.collect();

        let mut names = BTreeSet::new();
        for point in &points {
            names.insert(point.name.as_str());
        }
        let mut expected = BTreeSet::new();
        for (name, _, _, _) in METRICS {
            expected.insert(name);
        }
        assert_eq!(names, expected);
        for (name, kind, unit, keys) in METRICS {
            for point in named(&points, name) {
                assert_eq!(point.kind, kind, "{name}");
                assert_eq!(point.unit, unit, "{name}");
                let mut wanted: BTreeSet<&str> = keys.iter().copied().collect();
                if name == "milter.server.connection.closed" {
                    if let Some(reason) = point.attributes.get("milter.close_reason") {
                        if reason == "error" {
                            wanted.insert("error.type");
                        }
                    }
                }
                let mut got = BTreeSet::new();
                for key in point.attributes.keys() {
                    got.insert(key.as_str());
                }
                assert_eq!(got, wanted, "{name}: {point:?}");
            }
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn tel_011_counters_and_gauges_equal_status() {
        let metrics = Metrics::new();
        let server = busy_run(&metrics).await;

        let points = metrics.collect();
        let status = server.handle.status();

        let listener = &status.listeners[0];
        let address = listener.address.ip().to_string();
        let port = listener.address.port().to_string();
        let at = [
            ("server.address", address.as_str()),
            ("server.port", port.as_str()),
        ];
        assert_eq!(
            value(&points, "milter.server.connection.accepted", &at),
            Some(listener.connections_accepted as f64)
        );
        assert_eq!(
            value(&points, "milter.server.connection.active", &at),
            Some(listener.connections_active as f64)
        );
        assert_eq!(
            value(&points, "milter.server.connection.rejected", &at),
            Some(listener.connections_rejected as f64)
        );
        for (reason, error, count) in &status.connections_closed {
            let mut attributes = vec![("milter.close_reason", *reason)];
            if let Some(error) = error {
                attributes.push(("error.type", error));
            }
            assert_eq!(
                value(&points, "milter.server.connection.closed", &attributes),
                Some(*count as f64),
                "{reason}"
            );
        }
        assert_eq!(
            named(&points, "milter.server.connection.closed").len(),
            status.connections_closed.len()
        );
        assert_eq!(
            value(
                &points,
                "milter.server.io",
                &[("network.io.direction", "receive")]
            ),
            Some(status.bytes_received as f64)
        );
        assert_eq!(
            value(
                &points,
                "milter.server.io",
                &[("network.io.direction", "transmit")]
            ),
            Some(status.bytes_sent as f64)
        );
        for (version, count) in &status.negotiations {
            let version = version.to_string();
            assert_eq!(
                value(
                    &points,
                    "milter.server.negotiation",
                    &[("milter.protocol.version", &version)]
                ),
                Some(*count as f64)
            );
        }
        for (command, count) in &status.commands {
            assert_eq!(
                value(
                    &points,
                    "milter.server.command",
                    &[("milter.command", command)]
                ),
                Some(*count as f64),
                "{command}"
            );
        }
        assert_eq!(
            named(&points, "milter.server.command").len(),
            status.commands.len()
        );
        for (command, reply, count) in &status.replies {
            assert_eq!(
                value(
                    &points,
                    "milter.server.reply",
                    &[("milter.command", command), ("milter.reply", reply)]
                ),
                Some(*count as f64),
                "{command} {reply}"
            );
        }
        assert_eq!(
            named(&points, "milter.server.reply").len(),
            status.replies.len()
        );
        assert_eq!(
            value(&points, "milter.config.generation", &[]),
            Some(status.config_generation as f64)
        );
        for (result, source, count) in &status.reloads {
            assert_eq!(
                value(
                    &points,
                    "milter.config.reload",
                    &[
                        ("milter.reload_result", result),
                        ("milter.reload_source", source)
                    ]
                ),
                Some(*count as f64)
            );
        }
        let last = status.last_reload.expect("a reload was applied");
        let seconds = last.duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        let reported = value(&points, "milter.config.last_reload.time", &[]).unwrap();
        assert!((reported - seconds).abs() < 1.0, "{reported} {seconds}");
        server.stop().await;
    }

    #[tokio::test]
    async fn tel_012_histograms_use_the_listed_buckets() {
        let metrics = Metrics::new();
        let server = busy_run(&metrics).await;

        let points = metrics.collect();

        let buckets = [
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0,
        ];
        let mut histograms = 0;
        for point in &points {
            if point.kind == "histogram" {
                assert_eq!(point.bounds, buckets, "{}", point.name);
                histograms += 1;
            }
        }
        assert!(histograms > 0);
        server.stop().await;
    }

    #[tokio::test]
    async fn tel_013_a_protocol_error_is_a_close_with_its_error_type() {
        let metrics = Metrics::new();
        let server = start(&metrics, support::config(), ContinueAll).await;
        let mut closed = server.mta().await;
        closed.negotiate().await;
        closed.close().await;
        let mut broken = server.mta().await;
        broken.negotiate().await;
        broken.send(b'Z', Bytes::new()).await;
        assert!(broken.closes().await);
        server.idle().await;

        let points = metrics.collect();

        let mut sets = BTreeSet::new();
        for point in named(&points, "milter.server.connection.closed") {
            let set: Vec<(String, String)> = point.attributes.clone().into_iter().collect();
            sets.insert(set);
        }
        let expected: BTreeSet<Vec<(String, String)>> = [
            vec![("milter.close_reason".to_owned(), "client_closed".to_owned())],
            vec![
                ("error.type".to_owned(), "Protocol".to_owned()),
                ("milter.close_reason".to_owned(), "error".to_owned()),
            ],
        ]
        .into();
        assert_eq!(sets, expected);
        server.stop().await;
    }

    #[tokio::test]
    async fn tel_014_a_rejected_connection_is_counted_twice() {
        let metrics = Metrics::new();
        let server = start(&metrics, one_connection(), ContinueAll).await;
        let mut open = server.mta().await;
        open.negotiate().await;
        let mut second = server.mta().await;
        assert!(second.closes().await);
        support::wait_until(&server.handle, Until::Rejected(1)).await;

        let points = metrics.collect();

        let mut rejected = 0.0;
        for point in named(&points, "milter.server.connection.rejected") {
            rejected += point.value;
        }
        assert_eq!(rejected, 1.0);
        assert_eq!(
            value(
                &points,
                "milter.server.connection.closed",
                &[("milter.close_reason", "limit_reached")]
            ),
            Some(1.0)
        );
        drop(open);
        server.stop().await;
    }

    #[tokio::test]
    async fn tel_015_message_histograms_count_messages_not_chunks() {
        let metrics = Metrics::new();
        let server = start(&metrics, support::config(), ContinueAll).await;
        let mut mta = server.mta().await;
        mta.reach(Reach::Eohs).await;
        for _ in 0..50 {
            mta.ask_continue(SMFIC_BODY, Bytes::from_static(b"chunk\r\n"))
                .await;
        }
        mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
        mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;

        let points = metrics.collect();

        for name in [
            "milter.server.message.duration",
            "milter.server.eom.duration",
        ] {
            let mut count = 0;
            for point in named(&points, name) {
                count += point.count;
            }
            assert_eq!(count, 2, "{name}");
        }
        drop(mta);
        server.stop().await;
    }

    /// A loader that returns the prepared server configurations in turn.
    struct PreparedLoader {
        results: std::sync::Arc<std::sync::Mutex<Vec<ServerConfig>>>,
    }

    impl rust_milter::ReloadLoader<()> for PreparedLoader {
        type Error = io::Error;

        async fn load(&self) -> io::Result<Reloaded<()>> {
            let server = self.results.lock().unwrap().remove(0);
            Ok(Reloaded { server, app: () })
        }
    }

    /// The value of `milter.config.reload` for `result` and `source`.
    fn reloads(points: &[Point], result: &str, source: &str) -> Option<f64> {
        value(
            points,
            "milter.config.reload",
            &[
                ("milter.reload_result", result),
                ("milter.reload_source", source),
            ],
        )
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tel_016_reload_metrics_count_by_result_and_source() {
        let metrics = Metrics::new();
        let lease = PortLease::acquire().await;
        let config = ServerConfig {
            bind_addresses: vec![lease.v4()],
            shutdown: support::quick_shutdown(),
            ..ServerConfig::default()
        };
        let applied = ServerConfig {
            max_connections: 500,
            ..config.clone()
        };
        let rejected = ServerConfig {
            backlog: 2048,
            ..config.clone()
        };
        let results = std::sync::Arc::new(std::sync::Mutex::new(vec![applied, rejected]));
        let server = Server::builder()
            .config(config)
            .meter_provider(metrics.provider.clone())
            .reload_with(PreparedLoader { results })
            .handler(ContinueAll::new)
            .build()
            .unwrap();
        let handle = server.handle();
        let task = tokio::spawn(server.run());
        support::wait_until(&handle, Until::Accepting).await;

        let before = SystemTime::now();
        let status = Command::new("kill")
            .args(["-s", "HUP", &std::process::id().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
        support::wait_until(&handle, Until::Generation(2)).await;
        handle.reload().await.unwrap_err();

        let points = metrics.collect();

        assert_eq!(reloads(&points, "applied", "sighup"), Some(1.0));
        assert_eq!(reloads(&points, "failed", "api"), Some(1.0));
        assert_eq!(value(&points, "milter.config.generation", &[]), Some(2.0));
        let last = value(&points, "milter.config.last_reload.time", &[]).unwrap();
        let since = before.duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        assert!(
            last >= since.floor() && last < since + 5.0,
            "{last} {since}"
        );
        handle.shutdown();
        task.await.unwrap().unwrap();
    }

    /// The number of points of each metric.
    fn series(points: &[Point]) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for point in points {
            *counts.entry(point.name.clone()).or_default() += 1;
        }
        counts
    }

    /// Connects to `server`, negotiates, and closes.
    async fn negotiate_and_close(server: &Running) {
        let mut mta = server.mta().await;
        mta.negotiate().await;
        mta.close().await;
    }

    #[tokio::test]
    async fn tel_017_many_peers_add_no_time_series() {
        let metrics = Metrics::new();
        let server = start(&metrics, support::config(), ContinueAll).await;
        negotiate_and_close(&server).await;
        server.idle().await;
        let after_one = series(&metrics.collect());

        for _ in 1..10_000 {
            let stream = tokio::net::TcpStream::connect(server.lease.v4())
                .await
                .unwrap();
            drop(stream);
        }
        negotiate_and_close(&server).await;
        server.idle().await;
        let after_many = series(&metrics.collect());

        assert_eq!(after_one, after_many);
        assert_eq!(
            server.handle.status().listeners[0].connections_accepted,
            10_001
        );
        server.stop().await;
    }

    /// Values that must not appear in any metric.
    const MARKERS: [&str; 6] = [
        "marker-host.void.invalid",
        "192.0.2.77",
        "marker-helo.void.invalid",
        "marker-sender@void.invalid",
        "marker-header-value",
        "marker-reply-text",
    ];

    /// Replies with a custom text at end of message.
    struct Replying;

    impl Handler for Replying {
        type Session = ();
        type Message = ();

        async fn header(
            &self,
            _ctx: &mut MessageContext<Self>,
            _name: &MilterText,
            _value: &MilterText,
        ) -> Result<Decision> {
            Ok(Decision::Continue)
        }

        async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
            let reply = rust_milter::ReplyCode::new(550, Some("5.7.1"), "marker-reply-text")
                .expect("a valid reply");
            Ok(Decision::Reply(reply))
        }
    }

    #[tokio::test]
    async fn tel_032_no_smtp_data_reaches_metrics() {
        let metrics = Metrics::new();
        let server = start(&metrics, support::config(), Replying).await;
        let mut mta = server.mta().await;
        mta.negotiate().await;
        mta.ask_continue(
            SMFIC_CONNECT,
            connect_data("marker-host.void.invalid", "192.0.2.77", 4714),
        )
        .await;
        mta.ask_continue(SMFIC_HELO, strings(&["marker-helo.void.invalid"]))
            .await;
        mta.ask_continue(SMFIC_MAIL, strings(&["<marker-sender@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_HEADER, strings(&["X-Marker", "marker-header-value"]))
            .await;
        mta.ask_continue(SMFIC_EOH, Bytes::new()).await;
        let reply = mta.ask(SMFIC_BODYEOB, Bytes::new()).await;
        assert_eq!(reply.command, SMFIR_REPLYCODE);
        mta.close().await;
        server.idle().await;

        let points = metrics.collect();

        let text = format!("{points:?}");
        for marker in MARKERS {
            assert!(!text.contains(marker), "{marker} in {text}");
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn tel_003_without_an_sdk_a_session_still_works() {
        let server = TestServer::start(ContinueAll).await;
        let mut mta = server.mta().await;
        mta.reach(Reach::Endm).await;
        mta.close().await;
        support::wait_until(&server.handle, Until::Idle).await;

        let status = server.handle.status();

        assert_eq!(status.connections_accepted, 1);
        let mut eom_commands = 0;
        for (command, _) in &status.commands {
            if *command == "eom" {
                eom_commands += 1;
            }
        }
        assert!(eom_commands > 0);
        let mut eom_replies = 0;
        for (command, _, _) in &status.replies {
            if *command == "eom" {
                eom_replies += 1;
            }
        }
        assert!(eom_replies > 0);
        server.stop().await;
    }
}
