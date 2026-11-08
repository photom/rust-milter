//! Captures the `tracing` events the server emits on the test's thread.
//!
//! One global subscriber is installed for the whole test binary, and it
//! hands each event to the capture of the thread that emitted it. A
//! thread-local subscriber per test would miss events: `tracing` caches
//! whether a callsite is wanted, and a test thread without a subscriber can
//! set that cache to "never" for everyone. The tests run on Tokio's
//! current-thread runtime, so the server's tasks run on the test's thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, Once};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};

thread_local! {
    static CURRENT: RefCell<Option<Logs>> = const { RefCell::new(None) };
}

static INSTALL: Once = Once::new();

/// Ends the capture on this thread when dropped.
pub struct CaptureGuard;

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CURRENT.take();
    }
}

/// One captured event.
#[derive(Debug, Clone)]
pub struct Captured {
    pub level: Level,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

impl Captured {
    /// A field's value, as `tracing` formats it.
    pub fn field(&self, name: &str) -> Option<&str> {
        match self.fields.get(name) {
            Some(value) => Some(value.as_str()),
            None => None,
        }
    }
}

/// The events captured so far.
#[derive(Clone, Default)]
pub struct Logs(Arc<Mutex<Vec<Captured>>>);

impl Logs {
    /// Captures this thread's events until the guard is dropped.
    pub fn capture() -> (Logs, CaptureGuard) {
        INSTALL.call_once(|| {
            let subscriber = tracing_subscriber::registry().with(Forward);
            tracing::subscriber::set_global_default(subscriber)
                .expect("no other global subscriber in the tests");
        });
        let logs = Logs::default();
        CURRENT.set(Some(logs.clone()));
        (logs, CaptureGuard)
    }

    pub fn all(&self) -> Vec<Captured> {
        self.0.lock().unwrap().clone()
    }

    /// The events with this message.
    pub fn named(&self, message: &str) -> Vec<Captured> {
        let mut named = Vec::new();
        for event in self.all() {
            if event.message == message {
                named.push(event);
            }
        }
        named
    }

    /// The one event with this message; panics if there is not exactly one.
    pub fn one(&self, message: &str) -> Captured {
        let mut found = self.named(message);
        assert_eq!(found.len(), 1, "{message:?} in {:?}", self.all());
        found.remove(0)
    }

    /// The events above DEBUG: INFO is allowed, WARN and ERROR are not.
    pub fn warnings(&self) -> Vec<Captured> {
        let mut warnings = Vec::new();
        for event in self.all() {
            if event.level == Level::WARN || event.level == Level::ERROR {
                warnings.push(event);
            }
        }
        warnings
    }
}

/// Hands each event to the capture of the thread that emitted it.
struct Forward;

impl<S: Subscriber> Layer<S> for Forward {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let current = CURRENT.take();
        CURRENT.set(current.clone());
        let Some(logs) = current else {
            return;
        };
        let mut fields = Fields::default();
        event.record(&mut fields);
        let message = fields.0.remove("message").unwrap_or_default();
        logs.0.lock().unwrap().push(Captured {
            level: *event.metadata().level(),
            message,
            fields: fields.0,
        });
    }
}

#[derive(Default)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
