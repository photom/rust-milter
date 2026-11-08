//! The accept loop and its back-off after errors. See "Error recovery" in
//! `docs/reference/tcp-server.md`.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

/// The first pause after an accept error.
const FIRST_PAUSE: Duration = Duration::from_millis(100);
/// The longest pause; the pause doubles up to it.
const MAX_PAUSE: Duration = Duration::from_secs(1);

/// Something that accepts connections: a TCP listener, or a fake in tests.
pub(super) trait Listen: Send {
    type Stream: Send;

    fn accept(&mut self) -> impl Future<Output = io::Result<(Self::Stream, SocketAddr)>> + Send;
}

impl Listen for TcpListener {
    type Stream = TcpStream;

    async fn accept(&mut self) -> io::Result<(TcpStream, SocketAddr)> {
        TcpListener::accept(self).await
    }
}

/// What takes on the connections an accept loop accepts.
pub(super) trait OnAccept<S> {
    fn accepted(&mut self, stream: S, peer: SocketAddr);
}

/// The pause after each consecutive accept error.
#[derive(Debug, Default)]
struct Backoff {
    consecutive_errors: u32,
}

impl Backoff {
    /// Counts an error and returns the pause before the next accept: 100 ms,
    /// doubling up to 1 s.
    fn error(&mut self) -> Duration {
        self.consecutive_errors += 1;
        let doublings = self.consecutive_errors.min(5) - 1;
        (FIRST_PAUSE * 2u32.pow(doublings)).min(MAX_PAUSE)
    }

    fn reset(&mut self) {
        self.consecutive_errors = 0;
    }
}

/// Accepts connections on `listener` until `shutdown`, and hands each one to
/// `on_accept`.
///
/// `ECONNABORTED`, a client that gave up before `accept()` returned, is
/// retried at once. Every other error, such as `EMFILE` or `ENFILE`, pauses
/// the loop with [`Backoff`], so a failing listener never spins.
pub(super) async fn accept_loop<L: Listen, O: OnAccept<L::Stream>>(
    mut listener: L,
    address: SocketAddr,
    shutdown: &CancellationToken,
    on_accept: &mut O,
) {
    let mut backoff = Backoff::default();
    loop {
        let accepted = tokio::select! {
            () = shutdown.cancelled() => return,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((stream, peer)) => {
                backoff.reset();
                on_accept.accepted(stream, peer);
            }
            Err(error) if error.kind() == io::ErrorKind::ConnectionAborted => {
                tracing::debug!(listener = %address, error = %error, "Accept error");
            }
            Err(error) => {
                let pause = backoff.error();
                tracing::warn!(
                    listener = %address,
                    error = %error,
                    consecutive_errors = backoff.consecutive_errors,
                    "Accept error"
                );
                tokio::select! {
                    () = shutdown.cancelled() => return,
                    () = tokio::time::sleep(pause) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use tokio::time::Instant;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    /// A listener that returns scripted results, then waits forever, and
    /// records when each accept was called.
    struct Fake {
        script: VecDeque<io::Result<()>>,
        calls: Arc<Mutex<Vec<Instant>>>,
    }

    impl Listen for Fake {
        type Stream = ();

        async fn accept(&mut self) -> io::Result<((), SocketAddr)> {
            self.calls.lock().unwrap().push(Instant::now());
            match self.script.pop_front() {
                Some(Ok(())) => Ok(((), "192.0.2.9:4711".parse().unwrap())),
                Some(Err(error)) => Err(error),
                None => std::future::pending().await,
            }
        }
    }

    /// Counts the connections handed on.
    #[derive(Default)]
    struct Count(usize);

    impl OnAccept<()> for Count {
        fn accepted(&mut self, (): (), _peer: SocketAddr) {
            self.0 += 1;
        }
    }

    const EMFILE: i32 = 24;
    const ENFILE: i32 = 23;

    fn os_error(code: i32) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(code))
    }

    /// Runs the loop over `script` until it waits, and returns the pauses
    /// between the accept calls, how many connections were handed on, and
    /// the levels and `consecutive_errors` of the "Accept error" logs.
    async fn run(
        script: Vec<io::Result<()>>,
    ) -> (Vec<Duration>, usize, Vec<(tracing::Level, String)>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let length = script.len();
        let fake = Fake {
            script: script.into(),
            calls: Arc::clone(&calls),
        };
        let logs = Arc::new(Mutex::new(Vec::new()));
        let _guard = capture(Arc::clone(&logs));
        let shutdown = CancellationToken::new();
        let mut accepted = Count::default();

        let looping = accept_loop(
            fake,
            "127.0.0.1:61000".parse().unwrap(),
            &shutdown,
            &mut accepted,
        );
        // The paused clock advances through the pauses; the last accept
        // waits forever, so the timeout ends the run.
        let _ = tokio::time::timeout(Duration::from_secs(60), looping).await;

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), length + 1, "every scripted result was used");
        let mut pauses = Vec::new();
        for pair in calls.windows(2) {
            pauses.push(pair[1] - pair[0]);
        }
        let logs = logs.lock().unwrap().clone();
        (pauses, accepted.0, logs)
    }

    type Captured = Arc<Mutex<Vec<(tracing::Level, String)>>>;

    thread_local! {
        static CURRENT: std::cell::RefCell<Option<Captured>> = const { std::cell::RefCell::new(None) };
    }

    /// Ends the capture on this thread when dropped.
    struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            CURRENT.take();
        }
    }

    /// Captures this thread's events through one global subscriber; see
    /// `tests/support/logs.rs` for why it is not thread-local.
    fn capture(logs: Captured) -> Guard {
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| {
            let subscriber = tracing_subscriber::registry().with(Capture);
            tracing::subscriber::set_global_default(subscriber).unwrap();
        });
        CURRENT.set(Some(logs));
        Guard
    }

    struct Capture;

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let current = CURRENT.take();
            CURRENT.set(current.clone());
            let Some(logs) = current else {
                return;
            };
            struct Errors(String);
            impl tracing::field::Visit for Errors {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "consecutive_errors" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut errors = Errors(String::new());
            event.record(&mut errors);
            logs.lock()
                .unwrap()
                .push((*event.metadata().level(), errors.0));
        }
    }

    fn ms(values: &[u64]) -> Vec<Duration> {
        let mut pauses = Vec::new();
        for &value in values {
            pauses.push(Duration::from_millis(value));
        }
        pauses
    }

    #[tokio::test(start_paused = true)]
    async fn srv_053_backs_off_after_emfile_and_resets_after_an_accept() {
        let script = vec![
            os_error(EMFILE),
            os_error(EMFILE),
            os_error(EMFILE),
            os_error(EMFILE),
            Ok(()),
            os_error(EMFILE),
        ];

        let (pauses, accepted, logs) = run(script).await;

        assert_eq!(pauses, ms(&[100, 200, 400, 800, 0, 100]));
        assert_eq!(accepted, 1);
        let mut warnings = Vec::new();
        for (level, count) in &logs {
            if *level == tracing::Level::WARN {
                warnings.push(count.as_str());
            }
        }
        assert_eq!(warnings, ["1", "2", "3", "4", "1"]);
    }

    #[tokio::test(start_paused = true)]
    async fn srv_054_the_pause_stays_at_one_second() {
        let mut script = Vec::new();
        for _ in 0..10 {
            script.push(os_error(ENFILE));
        }

        let (pauses, _, _) = run(script).await;

        assert_eq!(
            pauses,
            ms(&[100, 200, 400, 800, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000])
        );
    }

    #[tokio::test(start_paused = true)]
    async fn srv_055_retries_at_once_after_econnaborted() {
        let script = vec![Err(io::ErrorKind::ConnectionAborted.into()), Ok(())];

        let (pauses, accepted, logs) = run(script).await;

        assert_eq!(pauses, ms(&[0, 0]));
        assert_eq!(accepted, 1);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, tracing::Level::DEBUG);
    }
}
