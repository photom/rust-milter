//! The interoperability scenarios (`INT`) from
//! `docs/testing/interoperability.md`.
//!
//! The tests are ignored outside the interoperability stack. Inside it, the
//! runner runs them with `--ignored --test-threads=1`. They are one test
//! binary, so that the SMTP sink runs for the whole run.

#![allow(
    clippy::manual_map,
    reason = "the code is procedural: a match is preferred to Option::map"
)]

mod basic_flow;
mod decisions;
mod lifecycle;
mod modifications;
mod protocol;
mod timeouts;
