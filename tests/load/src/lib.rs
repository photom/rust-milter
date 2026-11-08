//! The load generator of the load and chaos tests (`LOD` and `CHS`
//! scenarios).
//!
//! The tests run inside the Docker Compose stack in `tests/load/compose.yaml`;
//! see `docs/reference/test-environment.md` and testing ADR 0005. Each test
//! starts the milter under test as a child process with [`Milter::start`],
//! drives it with [`Mta`] connections, and reads its state with
//! [`Milter::status`] and from `/proc`.

pub mod churn;
pub mod milter;
pub mod mode;
pub mod mta;

pub use churn::{Churn, churn};
pub use milter::{Milter, Snapshot};
pub use mode::Mode;
pub use mta::{Failure, Mta};
