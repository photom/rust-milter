//! The interoperability scenarios (`INT`) from
//! `docs/testing/interoperability.md`.
//!
//! The tests are ignored outside the interoperability stack. Inside it, the
//! runner runs them with `--ignored --test-threads=1`. They are one test
//! binary, so that the SMTP sink runs for the whole run.

mod basic_flow;
mod decisions;
mod modifications;
