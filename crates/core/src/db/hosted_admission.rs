//! Transient receiving-host admission, separate from durable generation fences.
//!
//! A replayed fence can be older than current control authority. Every database
//! open starts closed, including cold maintenance opens and restored databases.
//! Trusted platform code must reconcile the complete incoming fence inventory
//! before completing a sweep. This state is never serialized or restored.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::ensure;
use parking_lot::Mutex;

#[derive(Default)]
struct State {
    revision: u64,
    fence_revision: u64,
    sweeping: bool,
}

#[derive(Default)]
struct Inner {
    open: AtomicBool,
    state: Mutex<State>,
}

/// Owned by one live RelationalDB. Opening another copy of the same database
/// Identity creates an independent, closed gate.
#[derive(Default)]
pub struct HostedAdmission(Arc<Inner>);

impl HostedAdmission {
    pub fn is_open(&self) -> bool {
        self.0.open.load(Ordering::Acquire)
    }

    /// Start one bounded reconciliation. A competing sweep fails immediately.
    /// Dropping its ticket keeps admission closed and permits a later retry.
    pub fn begin(&self) -> anyhow::Result<HostedAdmissionSweep> {
        let mut state = self.0.state.lock();
        ensure!(!state.sweeping, "hosted admission reconciliation is already running");
        self.0.open.store(false, Ordering::Release);
        let revision = state
            .revision
            .checked_add(1)
            .filter(|value| *value != u64::MAX)
            .ok_or_else(|| anyhow::anyhow!("hosted admission revision exhausted"))?;
        state.revision = revision;
        state.sweeping = true;
        Ok(HostedAdmissionSweep {
            inner: self.0.clone(),
            revision,
            fence_revision: state.fence_revision,
        })
    }

    /// Read while holding the database transaction that observes fence rows.
    pub fn fence_revision(&self) -> u64 {
        self.0.state.lock().fence_revision
    }

    /// Call before changing a durable fence, within its mutation transaction.
    /// Even a later rollback conservatively invalidates an inventory scan. A
    /// delayed installation after startup closes admission again, so it cannot
    /// insert an unexamined allowed row behind a completed scan's cursor.
    pub fn fences_changed(&self) -> anyhow::Result<(u64, u64)> {
        let mut state = self.0.state.lock();
        self.0.open.store(false, Ordering::Release);
        let previous = state.fence_revision;
        let Some(next) = previous.checked_add(1).filter(|next| *next != u64::MAX) else {
            state.fence_revision = u64::MAX;
            state.revision = u64::MAX;
            state.sweeping = false;
            anyhow::bail!("hosted fence revision exhausted");
        };
        state.fence_revision = next;
        Ok((previous, next))
    }

    /// Invalidate any outstanding sweep. This prevents new admission; it is
    /// not a substitute for a transactional fence and positive actor drainage.
    pub fn close(&self) {
        let mut state = self.0.state.lock();
        self.0.open.store(false, Ordering::Release);
        state.revision = state.revision.saturating_add(1);
        state.sweeping = false;
    }

    /// Permanently close a database whose storage writer is being shut down.
    /// A retained handle cannot start a new sweep on that obsolete object.
    pub fn seal(&self) {
        let mut state = self.0.state.lock();
        self.0.open.store(false, Ordering::Release);
        state.revision = u64::MAX;
        state.sweeping = false;
    }
}

/// A ticket is tied to the exact database-open state that issued it. Completing
/// one cannot open a replacement database, or undo a later close operation.
#[must_use = "dropping the sweep leaves hosted admission closed"]
pub struct HostedAdmissionSweep {
    inner: Arc<Inner>,
    revision: u64,
    fence_revision: u64,
}

impl HostedAdmissionSweep {
    /// Retain this guard in the async owner when moving the ticket to physical
    /// blocking work. Cancelling that owner invalidates this exact sweep, even
    /// when dropping the blocking task's JoinHandle cannot stop its execution.
    pub fn cancellation_guard(&self) -> HostedAdmissionCancellation {
        HostedAdmissionCancellation {
            inner: Some(self.inner.clone()),
            revision: self.revision,
        }
    }

    /// Trusted platform code calls this only after durable fence installation,
    /// actor completion and current control confirmation of the entire sweep.
    pub fn complete(self) -> anyhow::Result<()> {
        let revision = self.fence_revision;
        self.complete_with_fence_revision(revision)
    }

    /// Complete a scan which made its own confirmed fence mutations. The
    /// caller holds the actual DB transaction and has verified every stored
    /// fence against current authority at this exact physical revision.
    pub fn complete_with_fence_revision(self, fence_revision: u64) -> anyhow::Result<()> {
        let mut state = self.inner.state.lock();
        ensure!(
            state.sweeping && state.revision == self.revision,
            "hosted admission reconciliation was invalidated"
        );
        ensure!(
            fence_revision != u64::MAX && state.fence_revision == fence_revision,
            "hosted fence inventory changed during reconciliation"
        );
        state.sweeping = false;
        self.inner.open.store(true, Ordering::Release);
        Ok(())
    }
}

/// Cancellation belongs to one sweep revision. A late guard cannot close a
/// newer retry, and completion and cancellation serialize on the same mutex.
#[must_use = "retain until physical completion and disarm only after success"]
pub struct HostedAdmissionCancellation {
    inner: Option<Arc<Inner>>,
    revision: u64,
}

impl HostedAdmissionCancellation {
    pub fn disarm(mut self) {
        self.inner = None;
    }
}

impl Drop for HostedAdmissionCancellation {
    fn drop(&mut self) {
        let Some(inner) = &self.inner else { return };
        let mut state = inner.state.lock();
        if state.revision == self.revision {
            inner.open.store(false, Ordering::Release);
            state.revision = state.revision.saturating_add(1);
            state.sweeping = false;
        }
    }
}

impl Drop for HostedAdmissionSweep {
    fn drop(&mut self) {
        let mut state = self.inner.state.lock();
        if state.revision == self.revision {
            state.sweeping = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_sweep_retries_and_cannot_open_another_database() {
        let original = HostedAdmission::default();
        let replacement = HostedAdmission::default();
        assert!(!original.is_open());
        let sweep = original.begin().unwrap();
        assert!(original.begin().is_err());
        drop(sweep);
        assert!(!original.is_open());
        original.begin().unwrap().complete().unwrap();
        assert!(original.is_open());
        assert!(!replacement.is_open());
    }

    #[test]
    fn stale_completion_and_drop_cannot_open_or_cancel_a_newer_sweep() {
        let gate = HostedAdmission::default();
        let stale = gate.begin().unwrap();
        gate.close();
        let current = gate.begin().unwrap();
        assert!(stale.complete().is_err());
        assert!(!gate.is_open());
        assert!(gate.begin().is_err());
        current.complete().unwrap();
        assert!(gate.is_open());
        gate.close();
        assert!(!gate.is_open());
    }

    #[test]
    fn revision_exhaustion_remains_closed() {
        let gate = HostedAdmission::default();
        gate.0.state.lock().revision = u64::MAX - 2;
        gate.begin().unwrap().complete().unwrap();
        assert!(gate.is_open());
        assert!(gate.begin().is_err());
        assert!(!gate.is_open());
        gate.close();
        assert!(gate.begin().is_err());
        assert!(!gate.is_open());
    }

    #[test]
    fn owner_cancellation_invalidates_detached_completion_without_closing_a_newer_retry() {
        let gate = HostedAdmission::default();
        let stale = gate.begin().unwrap();
        let cancellation = stale.cancellation_guard();
        drop(cancellation);
        let current = gate.begin().unwrap();
        assert!(stale.complete().is_err());
        current.complete().unwrap();
        assert!(gate.is_open());

        gate.close();
        let stale = gate.begin().unwrap();
        let cancellation = stale.cancellation_guard();
        gate.close();
        gate.begin().unwrap().complete().unwrap();
        drop(cancellation);
        assert!(gate.is_open());
        assert!(stale.complete().is_err());
    }

    #[test]
    fn only_an_acknowledged_completion_survives_owner_drop() {
        let gate = HostedAdmission::default();
        let ticket = gate.begin().unwrap();
        let cancellation = ticket.cancellation_guard();
        ticket.complete().unwrap();
        drop(cancellation);
        assert!(!gate.is_open());
        let ticket = gate.begin().unwrap();
        let cancellation = ticket.cancellation_guard();
        ticket.complete().unwrap();
        cancellation.disarm();
        assert!(gate.is_open());
    }

    #[test]
    fn changed_fences_invalidate_scans_and_close_a_previously_open_gate() {
        let gate = HostedAdmission::default();
        let ticket = gate.begin().unwrap();
        assert_eq!(gate.fences_changed().unwrap(), (0, 1));
        assert!(ticket.complete().is_err());
        let ticket = gate.begin().unwrap();
        assert_eq!(gate.fences_changed().unwrap(), (1, 2));
        ticket.complete_with_fence_revision(2).unwrap();
        assert!(gate.is_open());
        gate.fences_changed().unwrap();
        assert!(!gate.is_open());
        let ticket = gate.begin().unwrap();
        gate.0.state.lock().fence_revision = u64::MAX - 1;
        assert!(gate.fences_changed().is_err());
        assert!(ticket.complete_with_fence_revision(u64::MAX).is_err());
        assert!(gate.begin().is_err());
    }
}
