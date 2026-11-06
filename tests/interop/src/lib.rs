//! Support code for the interoperability tests (`INT` scenarios).
//!
//! The tests run inside the Docker Compose stack in `tests/interop/compose.yaml`;
//! see `docs/reference/test-environment.md`. Each test starts a [`Milter`] on
//! port 8891 with the scenario's handler, sends a [`Mail`] through an [`Mta`]
//! with the [`Smtp`] client, and reads what reached the [`sink`] on port 2525.

pub mod logs;
pub mod mail;
pub mod milter;
pub mod queue;
pub mod sink;
pub mod smtp;
mod text;

use std::fmt;
use std::time::Duration;

pub use logs::Logs;
pub use mail::Mail;
pub use milter::Milter;
pub use sink::Delivered;
pub use smtp::{Reply, Smtp};

/// How long a test waits for a reply, a delivery, or the milter.
pub const WAIT: Duration = Duration::from_secs(30);

/// How long the first connection to an MTA may wait for the MTA to start.
pub const STARTUP: Duration = Duration::from_secs(120);

/// An MTA in the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mta {
    /// Postfix, listening on port 25 and the scenario ports.
    Postfix,
    /// Sendmail with the default filter timeouts.
    Sendmail,
    /// Sendmail with the filter timeouts `T=E:3s;R:2s`.
    SendmailShort,
}

impl Mta {
    /// The two MTAs a scenario for "both MTAs" runs with.
    pub const BOTH: [Mta; 2] = [Mta::Postfix, Mta::Sendmail];

    /// The MTA's host name in the Compose network.
    pub fn host(self) -> &'static str {
        match self {
            Mta::Postfix => "postfix",
            Mta::Sendmail => "sendmail",
            Mta::SendmailShort => "sendmail-short",
        }
    }
}

impl fmt::Display for Mta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.host())
    }
}
