//! The `max_memory` budget for the connections' read buffers. See
//! "Backpressure" in `docs/reference/tcp-server.md`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `max` when there is no limit.
const UNLIMITED: usize = usize::MAX;

/// What every connection reserves when it is accepted.
pub(crate) const BASE: usize = 4 * 1024;

/// The bytes reserved by all connections, against `max_memory`. A reload
/// can change the limit; reservations already made stay.
#[derive(Debug)]
pub(crate) struct Budget {
    /// `max_memory`, or [`UNLIMITED`]. No limit and a limit of
    /// `usize::MAX` bytes behave the same.
    max: AtomicUsize,
    used: AtomicUsize,
}

impl Budget {
    pub(crate) fn new(max: Option<usize>) -> Arc<Budget> {
        Arc::new(Budget {
            max: AtomicUsize::new(max.unwrap_or(UNLIMITED)),
            used: AtomicUsize::new(0),
        })
    }

    pub(crate) fn max(&self) -> Option<usize> {
        let max = self.max.load(Ordering::Relaxed);
        if max == UNLIMITED { None } else { Some(max) }
    }

    pub(crate) fn set_max(&self, max: Option<usize>) {
        self.max.store(max.unwrap_or(UNLIMITED), Ordering::Relaxed);
    }

    pub(crate) fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }

    /// Takes `bytes` if they fit under `max_memory`.
    fn take(&self, bytes: usize) -> bool {
        let max = self.max.load(Ordering::Relaxed);
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            // A sum that saturates fits only without a limit.
            let total = used.saturating_add(bytes);
            if total > max {
                return false;
            }
            match self
                .used
                .compare_exchange_weak(used, total, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(current) => used = current,
            }
        }
    }

    fn give(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::AcqRel);
    }
}

/// One connection's share of the budget, returned when it is dropped.
#[derive(Debug)]
pub(crate) struct Reservation {
    budget: Arc<Budget>,
    size: usize,
}

impl Reservation {
    /// Reserves [`BASE`] bytes for a new connection, if they fit.
    pub(crate) fn new(budget: &Arc<Budget>) -> Option<Reservation> {
        if !budget.take(BASE) {
            return None;
        }
        Some(Reservation {
            budget: Arc::clone(budget),
            size: BASE,
        })
    }

    /// Grows or shrinks the reservation to `bytes`, but never below
    /// [`BASE`]. Returns false, and keeps the old size, when growing would
    /// pass `max_memory`.
    pub(crate) fn resize(&mut self, bytes: usize) -> bool {
        let wanted = bytes.max(BASE);
        if wanted > self.size {
            if !self.budget.take(wanted - self.size) {
                return false;
            }
        } else {
            self.budget.give(self.size - wanted);
        }
        self.size = wanted;
        true
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.give(self.size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reservation_grows_within_the_budget_and_returns_on_drop() {
        let budget = Budget::new(Some(10 * 1024));
        let mut first = Reservation::new(&budget).unwrap();
        let second = Reservation::new(&budget).unwrap();

        assert!(!first.resize(7 * 1024), "4 + 7 KB passes 10 KB");
        assert!(first.resize(6 * 1024));
        assert!(Reservation::new(&budget).is_none());

        drop(second);
        first.resize(0);
        assert_eq!(budget.used(), BASE);
    }
}
