//! Load scenarios (`LOD`) of `docs/testing/load-and-chaos.md`. They run in
//! the Compose stack of `tests/load/compose.yaml`, one at a time, and are
//! ignored elsewhere.

use std::time::Duration;

use rust_milter_load::{Milter, Mode, churn};

/// The milter under test.
const LOAD_MILTER: &str = env!("CARGO_BIN_EXE_load-milter");

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs in the load stack: tests/load/compose.yaml"]
async fn lod_002_churn_of_1000_connections_per_second_leaves_no_file_open() {
    let duration = Mode::from_env().duration(Duration::from_secs(600), Duration::from_secs(60));
    let per_second = 1000;
    let mut milter = Milter::start(LOAD_MILTER, &[]);
    let before = milter.open_files();

    let churn = churn(milter.address(), per_second, duration).await;

    let expected = u64::from(per_second) * duration.as_secs();
    println!(
        "LOD-002: {} connections in {duration:?}, {} completed, {} failed",
        churn.started, churn.completed, churn.failed
    );
    assert!(
        churn.started * 100 >= expected * 99,
        "the generator opened {} of {expected} connections",
        churn.started
    );
    assert_eq!(
        churn.failed, 0,
        "every session completes; first failures: {:?}",
        churn.failures
    );
    let after = milter.wait_until_idle(Duration::from_secs(30));
    assert_eq!(
        after.accepted, churn.started,
        "the milter accepted every connection"
    );
    assert_eq!(
        milter.open_files(),
        before,
        "the open file descriptors return to their starting count"
    );
}
