//! Full or short runs: see "Load and chaos environment" in
//! `docs/reference/test-environment.md`.

use std::time::Duration;

/// The variable that selects the short mode.
const VARIABLE: &str = "RUST_MILTER_LOAD_SHORT";

/// How long the load scenarios run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The durations of the scenarios, as written.
    Full,
    /// Durations cut to a few minutes, for pull requests. Connection counts,
    /// rates, and pass criteria stay as written.
    Short,
}

impl Mode {
    /// The mode that `RUST_MILTER_LOAD_SHORT` selects: `on` is short, and
    /// `off` or no variable is full.
    ///
    /// # Panics
    ///
    /// On any other value, so that a typo does not run the wrong mode.
    pub fn from_env() -> Mode {
        let Ok(value) = std::env::var(VARIABLE) else {
            return Mode::Full;
        };
        match value.as_str() {
            "on" => Mode::Short,
            "off" => Mode::Full,
            other => panic!("{VARIABLE} is {other:?}; use on or off"),
        }
    }

    /// `full` in the full mode, `short` in the short mode.
    pub fn duration(self, full: Duration, short: Duration) -> Duration {
        match self {
            Mode::Full => full,
            Mode::Short => short,
        }
    }
}
