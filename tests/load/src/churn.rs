//! Connection churn: connections opened at a fixed rate, one session each
//! (LOD-002).

use std::net::SocketAddr;
use std::time::Duration;

use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};

use crate::mta::{Failure, Mta};

/// How many failures a [`Churn`] keeps to report.
const KEPT_FAILURES: usize = 10;

/// The outcome of [`churn`].
#[derive(Debug, Default)]
pub struct Churn {
    /// Connections opened.
    pub started: u64,
    /// Sessions that completed.
    pub completed: u64,
    /// Sessions that failed.
    pub failed: u64,
    /// The first failures, as text.
    pub failures: Vec<String>,
}

impl Churn {
    fn record(&mut self, outcome: Result<(), Failure>) {
        match outcome {
            Ok(()) => self.completed += 1,
            Err(failure) => {
                self.failed += 1;
                if self.failures.len() < KEPT_FAILURES {
                    self.failures.push(failure.to_string());
                }
            }
        }
    }

    fn record_join(&mut self, joined: Result<Result<(), Failure>, tokio::task::JoinError>) {
        match joined {
            Ok(outcome) => self.record(outcome),
            Err(error) => {
                self.failed += 1;
                if self.failures.len() < KEPT_FAILURES {
                    self.failures
                        .push(format!("the session task failed: {error}"));
                }
            }
        }
    }
}

/// Opens `per_second` connections per second to `address` for `duration`.
/// Each connection negotiates, runs one session, sends `SMFIC_QUIT`, and
/// closes. Returns once every session has ended.
pub async fn churn(address: SocketAddr, per_second: u32, duration: Duration) -> Churn {
    let mut ticker = tokio::time::interval(Duration::from_secs(1) / per_second);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Burst);
    let deadline = Instant::now() + duration;
    let mut sessions = JoinSet::new();
    let mut churn = Churn::default();
    loop {
        let tick = ticker.tick().await;
        if tick >= deadline {
            break;
        }
        sessions.spawn(one_session(address));
        churn.started += 1;
        while let Some(joined) = sessions.try_join_next() {
            churn.record_join(joined);
        }
    }
    while let Some(joined) = sessions.join_next().await {
        churn.record_join(joined);
    }
    churn
}

/// One connection with one session.
async fn one_session(address: SocketAddr) -> Result<(), Failure> {
    let mut mta = Mta::connect(address).await?;
    mta.negotiate().await?;
    mta.session().await?;
    mta.quit().await
}
