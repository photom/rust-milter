//! The reload coordinator: it runs one reload at a time and folds the
//! loader requests that arrive during a reload into one more reload. See
//! "Reload coordinator states" in `docs/reference/hot-reload.md`.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::status::{Counters, Source};
use crate::{ReloadError, ServerConfig};

/// A new server configuration and application configuration, returned by a
/// reload loader or passed to `ServerHandle::reload_with_config`.
#[derive(Debug, Clone, PartialEq)]
pub struct Reloaded<A> {
    /// The new server configuration.
    pub server: ServerConfig,
    /// The new application configuration.
    pub app: A,
}

/// The result of a reload that was applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReloadReport {
    /// The generation after the reload.
    pub generation: u64,
    /// The names of the changed settings.
    pub changed: Vec<&'static str>,
    /// The listeners the reload bound.
    pub listeners_added: Vec<SocketAddr>,
    /// The listeners the reload stopped.
    pub listeners_removed: Vec<SocketAddr>,
    /// The addresses that failed to bind in `BestEffort` mode, with the
    /// error.
    pub bind_failures: Vec<(SocketAddr, String)>,
}

/// The application's reload loader: it reads the new server and application
/// configurations each time a reload runs, on `SIGHUP` or
/// [`ServerHandle::reload`](crate::ServerHandle::reload). Register it with
/// [`ServerBuilder::reload_with`](crate::ServerBuilder::reload_with).
///
/// The loader only reads and checks. rust-milter validates and applies what
/// it returns, and can still reject the reload, so the loader must not
/// change state that other code reads.
pub trait ReloadLoader<A>: Send + Sync + 'static {
    /// Why a load failed. The reload then fails with
    /// [`ReloadError::Load`], which keeps this error as its source.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Reads the new configurations. Implement it as an `async fn`.
    fn load(&self) -> impl Future<Output = Result<Reloaded<A>, Self::Error>> + Send;
}

/// What a loader returns, with its error boxed.
pub(crate) type Loaded<A> = Result<Reloaded<A>, Box<dyn std::error::Error + Send + Sync>>;

/// The object-safe form of [`ReloadLoader`], so that the server can keep
/// a loader of any type.
pub(crate) trait DynLoader<A>: Send + Sync {
    /// Runs [`ReloadLoader::load`] and boxes its error.
    fn run(&self) -> Pin<Box<dyn Future<Output = Loaded<A>> + Send + '_>>;
}

impl<A: Send + 'static, L: ReloadLoader<A>> DynLoader<A> for L {
    fn run(&self) -> Pin<Box<dyn Future<Output = Loaded<A>> + Send + '_>> {
        Box::pin(load_boxed(self))
    }
}

/// Runs `loader` and boxes its error.
async fn load_boxed<A, L: ReloadLoader<A>>(loader: &L) -> Loaded<A> {
    match ReloadLoader::load(loader).await {
        Ok(reloaded) => Ok(reloaded),
        Err(error) => Err(Box::new(error)),
    }
}

/// The application's reload loader, as the server keeps it.
pub(crate) type Loader<A> = Box<dyn DynLoader<A>>;

/// The result of one reload request.
pub(super) type Outcome = Result<ReloadReport, ReloadError>;

/// A request to the coordinator.
pub(super) enum Request<A> {
    /// Run the loader. `SIGHUP` waits for no result, so it sends `None`.
    Load(Option<oneshot::Sender<Outcome>>),
    /// Apply a configuration the caller built.
    Apply(Box<Reloaded<A>>, oneshot::Sender<Outcome>),
}

/// What applies a validated configuration: the listeners and the config
/// channel of a running server.
pub(super) trait Target<A>: Send {
    /// Binds the added listeners, publishes `new` as `generation`, and stops
    /// the removed listeners; or rolls the binds back and fails.
    fn apply(&mut self, generation: u64, new: Reloaded<A>) -> impl Future<Output = Outcome> + Send;
}

/// Runs the reloads, one at a time, until shutdown.
pub(super) struct Coordinator<A, T> {
    loader: Option<Loader<A>>,
    target: T,
    generation: u64,
    stop: CancellationToken,
    /// Where `SIGHUP` requests and applied reloads are counted.
    counters: Arc<Counters>,
}

impl<A: Send + 'static, T: Target<A>> Coordinator<A, T> {
    pub(super) fn new(
        loader: Option<Loader<A>>,
        target: T,
        generation: u64,
        stop: CancellationToken,
        counters: Arc<Counters>,
    ) -> Coordinator<A, T> {
        Coordinator {
            loader,
            target,
            generation,
            stop,
            counters,
        }
    }

    /// Serves requests until shutdown starts. A reload that is applying
    /// when it starts finishes first; one that is loading is abandoned.
    /// Requests still queued then are dropped, and their callers get
    /// `ShuttingDown`.
    pub(super) async fn run(mut self, mut requests: mpsc::UnboundedReceiver<Request<A>>) {
        loop {
            let first = tokio::select! {
                biased;
                () = self.stop.cancelled() => return,
                request = requests.recv() => match request {
                    Some(request) => request,
                    None => return,
                },
            };
            let mut batch = vec![first];
            while let Ok(request) = requests.try_recv() {
                batch.push(request);
            }
            let mut loads = Vec::new();
            for request in batch {
                match request {
                    Request::Apply(new, reply) => {
                        let outcome = self.apply(*new).await;
                        let _ = reply.send(outcome);
                    }
                    Request::Load(reply) => loads.push(reply),
                }
            }
            if loads.is_empty() {
                continue;
            }
            if loads.len() > 1 {
                tracing::debug!(pending_requests = loads.len(), "Reload coalesced");
            }
            let outcome = self.load().await;
            // `reload()` counts its own requests; `SIGHUP` waits for none.
            for reply in &loads {
                if reply.is_none() {
                    self.counters.reloaded(outcome.is_ok(), Source::Sighup);
                }
            }
            answer(loads, outcome);
        }
    }

    /// Runs the loader, then applies what it returned.
    async fn load(&mut self) -> Outcome {
        let Some(loader) = &self.loader else {
            return Err(ReloadError::NoLoader);
        };
        let loaded = tokio::select! {
            biased;
            () = self.stop.cancelled() => return Err(ReloadError::ShuttingDown),
            loaded = loader.run() => loaded,
        };
        match loaded {
            Ok(new) => self.apply(new).await,
            Err(error) => Err(self.failed(ReloadError::Load(error))),
        }
    }

    /// Validates `new` and applies it as the next generation.
    async fn apply(&mut self, new: Reloaded<A>) -> Outcome {
        if self.stop.is_cancelled() {
            return Err(ReloadError::ShuttingDown);
        }
        if let Err(error) = new.server.validate() {
            return Err(self.failed(ReloadError::Validation(error)));
        }
        let next = self.generation + 1;
        match self.target.apply(next, new).await {
            Ok(report) => {
                self.generation = next;
                self.counters.applied_now();
                tracing::info!(
                    generation = report.generation,
                    changed = ?report.changed,
                    listeners_added = ?report.listeners_added,
                    listeners_removed = ?report.listeners_removed,
                    "Reload applied"
                );
                Ok(report)
            }
            Err(error) => Err(self.failed(error)),
        }
    }

    fn failed(&self, error: ReloadError) -> ReloadError {
        tracing::error!(
            generation = self.generation,
            error = %super::error_chain(&error),
            "Reload failed"
        );
        error
    }
}

/// Sends `outcome` to every caller of a folded reload: copies to all but the
/// last, which gets the original with its error's source types intact.
fn answer(mut replies: Vec<Option<oneshot::Sender<Outcome>>>, outcome: Outcome) {
    let last = replies.pop().flatten();
    for reply in replies.into_iter().flatten() {
        let _ = reply.send(copy(&outcome));
    }
    if let Some(reply) = last {
        let _ = reply.send(outcome);
    }
}

fn copy(outcome: &Outcome) -> Outcome {
    let error = match outcome {
        Ok(report) => return Ok(report.clone()),
        Err(error) => error,
    };
    Err(match error {
        ReloadError::NoLoader => ReloadError::NoLoader,
        ReloadError::Load(source) => ReloadError::Load(Box::new(Copied::of(&**source))),
        ReloadError::Validation(source) => ReloadError::Validation(source.clone()),
        ReloadError::RestartRequired { settings } => ReloadError::RestartRequired {
            settings: settings.clone(),
        },
        ReloadError::Bind { addr, source } => ReloadError::Bind {
            addr: *addr,
            source: std::io::Error::new(source.kind(), Copied::of(source)),
        },
        ReloadError::NoListener => ReloadError::NoListener,
        ReloadError::ShuttingDown => ReloadError::ShuttingDown,
    })
}

/// The text of an error and of its `source()` chain, for the copies of a
/// shared error.
#[derive(Debug)]
struct Copied {
    message: String,
    source: Option<Box<Copied>>,
}

impl Copied {
    fn of(error: &(dyn std::error::Error + 'static)) -> Copied {
        let mut copied = Copied {
            message: error.to_string(),
            source: None,
        };
        if let Some(source) = error.source() {
            copied.source = Some(Box::new(Copied::of(source)));
        }
        copied
    }
}

impl std::fmt::Display for Copied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Copied {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.source {
            Some(source) => Some(source.as_ref()),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::Semaphore;

    use super::*;
    use crate::ConfigError;

    /// A loader that waits for a permit before each call returns, and
    /// returns prepared configurations in turn.
    #[derive(Clone)]
    struct Gated {
        gate: Arc<Semaphore>,
        results: Arc<Mutex<VecDeque<ServerConfig>>>,
        calls: Arc<Mutex<usize>>,
    }

    impl Gated {
        fn new(results: Vec<ServerConfig>) -> Gated {
            Gated {
                gate: Arc::new(Semaphore::new(0)),
                results: Arc::new(Mutex::new(results.into())),
                calls: Arc::new(Mutex::new(0)),
            }
        }

        fn loader(&self) -> Loader<()> {
            Box::new(self.clone())
        }

        fn calls(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    impl ReloadLoader<()> for Gated {
        type Error = std::io::Error;

        async fn load(&self) -> Result<Reloaded<()>, std::io::Error> {
            *self.calls.lock().unwrap() += 1;
            self.gate.acquire().await.unwrap().forget();
            let server = self.results.lock().unwrap().pop_front().unwrap();
            Ok(Reloaded { server, app: () })
        }
    }

    /// A target that records the generations it published, and waits for
    /// a permit in each apply when it has a gate.
    #[derive(Clone, Default)]
    struct Recording {
        gate: Option<Arc<Semaphore>>,
        published: Arc<Mutex<Vec<u64>>>,
    }

    impl Target<()> for Recording {
        async fn apply(&mut self, generation: u64, _new: Reloaded<()>) -> Outcome {
            if let Some(gate) = &self.gate {
                gate.acquire().await.unwrap().forget();
            }
            self.published.lock().unwrap().push(generation);
            Ok(ReloadReport {
                generation,
                ..ReloadReport::default()
            })
        }
    }

    fn valid() -> ServerConfig {
        ServerConfig {
            bind_addresses: vec!["127.0.0.1:61001".parse().unwrap()],
            ..ServerConfig::default()
        }
    }

    struct Running {
        requests: mpsc::UnboundedSender<Request<()>>,
        stop: CancellationToken,
        task: tokio::task::JoinHandle<()>,
    }

    fn start(loader: Option<Loader<()>>, target: Recording) -> Running {
        let (requests, receiver) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        let counters = Arc::new(Counters::new(&[]));
        let coordinator = Coordinator::new(loader, target, 1, stop.clone(), counters);
        let task = tokio::spawn(coordinator.run(receiver));
        Running {
            requests,
            stop,
            task,
        }
    }

    impl Running {
        fn load(&self) -> oneshot::Receiver<Outcome> {
            let (reply, outcome) = oneshot::channel();
            self.requests.send(Request::Load(Some(reply))).unwrap();
            outcome
        }
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn rld_002_requests_during_a_reload_cause_one_more_reload() {
        // Enough for six loads, so a coordinator that folds nothing fails
        // on the count instead of waiting.
        let gated = Gated::new(vec![valid(); 6]);
        let running = start(Some(gated.loader()), Recording::default());

        let first = running.load();
        settle().await;
        let mut others = Vec::new();
        for _ in 0..5 {
            others.push(running.load());
        }
        settle().await;
        gated.gate.add_permits(6);

        assert_eq!(first.await.unwrap().unwrap().generation, 2);
        for other in others {
            assert_eq!(other.await.unwrap().unwrap().generation, 3);
        }
        assert_eq!(gated.calls(), 2);
    }

    #[tokio::test]
    async fn rld_003_a_failed_reload_keeps_the_generation() {
        let invalid = ServerConfig {
            backlog: 0,
            ..valid()
        };
        let gated = Gated::new(vec![valid(), invalid, valid()]);
        gated.gate.add_permits(3);
        let target = Recording::default();
        let running = start(Some(gated.loader()), target.clone());

        let first = running.load().await.unwrap();
        let second = running.load().await.unwrap();
        let third = running.load().await.unwrap();

        assert_eq!(first.unwrap().generation, 2);
        assert!(matches!(
            second,
            Err(ReloadError::Validation(ConfigError::OutOfRange {
                field: "backlog",
                ..
            }))
        ));
        assert_eq!(third.unwrap().generation, 3);
        assert_eq!(*target.published.lock().unwrap(), [2, 3]);
    }

    #[tokio::test]
    async fn rld_004_shutdown_abandons_a_reload_that_is_loading() {
        let gated = Gated::new(vec![valid()]);
        let target = Recording::default();
        let running = start(Some(gated.loader()), target.clone());

        let outcome = running.load();
        settle().await;
        running.stop.cancel();

        // The loader never returns: the coordinator stops waiting for it.
        let outcome = tokio::time::timeout(Duration::from_secs(1), outcome).await;
        assert!(matches!(outcome, Ok(Ok(Err(ReloadError::ShuttingDown)))));
        running.task.await.unwrap();
        assert!(target.published.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rld_005_shutdown_lets_a_reload_that_is_applying_finish() {
        let gated = Gated::new(vec![valid()]);
        gated.gate.add_permits(1);
        let gate = Arc::new(Semaphore::new(0));
        let target = Recording {
            gate: Some(Arc::clone(&gate)),
            ..Recording::default()
        };
        let running = start(Some(gated.loader()), target.clone());

        let outcome = running.load();
        settle().await;
        running.stop.cancel();
        settle().await;
        assert!(!running.task.is_finished(), "the apply is not interrupted");
        gate.add_permits(1);

        assert_eq!(outcome.await.unwrap().unwrap().generation, 2);
        running.task.await.unwrap();
        assert_eq!(*target.published.lock().unwrap(), [2]);
    }
}
