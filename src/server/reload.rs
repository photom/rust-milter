//! The reload coordinator: it runs one reload at a time and folds the
//! loader requests that arrive during a reload into one more reload. See
//! "Reload coordinator states" in `docs/reference/hot-reload.md`.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

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

/// What a loader returns.
pub(crate) type Loaded<A> = Result<Reloaded<A>, Box<dyn std::error::Error + Send + Sync>>;

/// The application's reload loader.
pub(crate) type Loader<A> =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = Loaded<A>> + Send>> + Send + Sync>;

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
}

impl<A: Send + 'static, T: Target<A>> Coordinator<A, T> {
    pub(super) fn new(
        loader: Option<Loader<A>>,
        target: T,
        generation: u64,
        stop: CancellationToken,
    ) -> Coordinator<A, T> {
        Coordinator {
            loader,
            target,
            generation,
            stop,
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
            loaded = loader() => loaded,
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
        Copied {
            message: error.to_string(),
            source: error.source().map(|source| Box::new(Copied::of(source))),
        }
    }
}

impl std::fmt::Display for Copied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Copied {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
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
            let gated = self.clone();
            Box::new(move || {
                let gated = gated.clone();
                Box::pin(async move {
                    *gated.calls.lock().unwrap() += 1;
                    gated.gate.acquire().await.unwrap().forget();
                    let server = gated.results.lock().unwrap().pop_front().unwrap();
                    Ok(Reloaded { server, app: () })
                })
            })
        }

        fn calls(&self) -> usize {
            *self.calls.lock().unwrap()
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
        let coordinator = Coordinator::new(loader, target, 1, stop.clone());
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
        let others: Vec<_> = (0..5).map(|_| running.load()).collect();
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
