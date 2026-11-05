//! Captures the `tracing` events of the milter under test.
//!
//! One global subscriber keeps every event in one list. The `INT` tests run
//! one at a time, so [`Logs::capture`] clears the list at the start of each
//! test, and the list holds that test's events only. Each event is also
//! printed, so the output of a failing test shows the milter's log.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Mutex, Once};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};

static EVENTS: Mutex<Vec<Captured>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();

/// One captured event.
#[derive(Debug, Clone)]
pub struct Captured {
    /// The event's level.
    pub level: Level,
    /// The event's message.
    pub message: String,
    /// The other fields, as `tracing` formats them.
    pub fields: BTreeMap<String, String>,
}

impl Captured {
    /// A field's value.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

/// The events captured since [`Logs::capture`].
#[derive(Debug)]
pub struct Logs(());

impl Logs {
    /// Installs the subscriber on first use and drops the events of earlier
    /// tests.
    pub fn capture() -> Logs {
        INSTALL.call_once(|| {
            let subscriber = tracing_subscriber::registry().with(Keep);
            tracing::subscriber::set_global_default(subscriber)
                .expect("no other global subscriber in the tests");
        });
        EVENTS.lock().unwrap().clear();
        Logs(())
    }

    /// Every event so far.
    pub fn all(&self) -> Vec<Captured> {
        EVENTS.lock().unwrap().clone()
    }

    /// The events with this message.
    pub fn named(&self, message: &str) -> Vec<Captured> {
        self.all()
            .into_iter()
            .filter(|e| e.message == message)
            .collect()
    }

    /// The WARN and ERROR events.
    pub fn warnings(&self) -> Vec<Captured> {
        self.at_least(Level::WARN)
    }

    /// The events at `level` or more severe.
    pub fn at_least(&self, level: Level) -> Vec<Captured> {
        // In `tracing`, a more severe level compares as smaller.
        self.all()
            .into_iter()
            .filter(|e| e.level <= level)
            .collect()
    }
}

/// Keeps and prints each event.
struct Keep;

impl<S: Subscriber> Layer<S> for Keep {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let message = fields.0.remove("message").unwrap_or_default();
        let captured = Captured {
            level: *event.metadata().level(),
            message,
            fields: fields.0,
        };
        println!(
            "milter log: {} {} {:?}",
            captured.level, captured.message, captured.fields
        );
        EVENTS.lock().unwrap().push(captured);
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
