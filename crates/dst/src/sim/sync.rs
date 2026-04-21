//! Tiny synchronization primitives for deterministic tests.
//!
//! This file models only the behavior needed by crate tests; it is not trying
//! to be a full synchronization library.

use std::collections::VecDeque;

/// Lock lifecycle events emitted by [`SimRwLock`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockEventKind {
    ReadRequested,
    WriteRequested,
    ReadGranted,
    WriteGranted,
    ReadReleased,
    WriteReleased,
}

/// One simulated lock event tagged with the actor that caused it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LockEvent {
    pub actor_id: usize,
    pub kind: LockEventKind,
}

/// Minimal FIFO read/write lock model used in deterministic tests.
#[derive(Clone, Debug, Default)]
pub struct SimRwLock {
    readers: usize,
    writer: Option<usize>,
    waiters: VecDeque<(usize, LockEventKind)>,
}

impl SimRwLock {
    pub fn request_read(&mut self, actor_id: usize) -> LockEvent {
        self.waiters.push_back((actor_id, LockEventKind::ReadRequested));
        LockEvent {
            actor_id,
            kind: LockEventKind::ReadRequested,
        }
    }

    pub fn request_write(&mut self, actor_id: usize) -> LockEvent {
        self.waiters.push_back((actor_id, LockEventKind::WriteRequested));
        LockEvent {
            actor_id,
            kind: LockEventKind::WriteRequested,
        }
    }

    pub fn grant_next(&mut self) -> Option<LockEvent> {
        let &(actor_id, kind) = self.waiters.front()?;
        match kind {
            LockEventKind::ReadRequested if self.writer.is_none() => {
                self.waiters.pop_front();
                self.readers += 1;
                Some(LockEvent {
                    actor_id,
                    kind: LockEventKind::ReadGranted,
                })
            }
            LockEventKind::WriteRequested if self.writer.is_none() && self.readers == 0 => {
                self.waiters.pop_front();
                self.writer = Some(actor_id);
                Some(LockEvent {
                    actor_id,
                    kind: LockEventKind::WriteGranted,
                })
            }
            _ => None,
        }
    }

    pub fn release_read(&mut self, actor_id: usize) -> LockEvent {
        assert!(self.readers > 0, "no reader to release");
        self.readers -= 1;
        LockEvent {
            actor_id,
            kind: LockEventKind::ReadReleased,
        }
    }

    pub fn release_write(&mut self, actor_id: usize) -> LockEvent {
        assert_eq!(self.writer, Some(actor_id), "actor does not own write lock");
        self.writer = None;
        LockEvent {
            actor_id,
            kind: LockEventKind::WriteReleased,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LockEventKind, SimRwLock};

    #[test]
    fn writer_waits_for_reader() {
        let mut lock = SimRwLock::default();
        lock.request_read(1);
        assert_eq!(lock.grant_next().unwrap().kind, LockEventKind::ReadGranted);

        lock.request_write(2);
        assert!(lock.grant_next().is_none());

        lock.release_read(1);
        assert_eq!(lock.grant_next().unwrap().kind, LockEventKind::WriteGranted);
    }
}
