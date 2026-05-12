use alloc::vec::Vec;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

use super::{TimeHandle, TimerId};

/// Future returned by [`TimeHandle::sleep`].
///
/// The future stores a relative duration until first poll, then converts that
/// into an absolute deadline and a stable timer id. Subsequent polls either
/// complete immediately if virtual time has already reached the deadline or
/// refresh the registered waker and remain pending.
pub struct Sleep {
    duration: Duration,
    state: SleepState,
}

impl Sleep {
    pub(super) fn new(handle: TimeHandle, duration: Duration) -> Self {
        Self {
            duration,
            state: SleepState::Unregistered { handle },
        }
    }
}

/// Internal state machine for [`Sleep`].
enum SleepState {
    Unregistered {
        handle: TimeHandle,
    },
    Registered {
        handle: TimeHandle,
        id: TimerId,
        deadline: Duration,
    },
    Done,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if matches!(self.state, SleepState::Done) {
            return Poll::Ready(());
        }

        if let SleepState::Unregistered { handle } = &self.state {
            let handle = handle.clone();
            let deadline = handle.now().saturating_add(self.duration);
            let id = handle.next_timer_id();
            self.state = SleepState::Registered { handle, id, deadline };
        }

        let SleepState::Registered { handle, id, deadline } = &self.state else {
            unreachable!("sleep state should be registered or done");
        };

        if handle.now() >= *deadline {
            let handle = handle.clone();
            let id = *id;
            handle.cancel_timer(id);
            self.state = SleepState::Done;
            Poll::Ready(())
        } else {
            handle.register_timer(*id, *deadline, cx.waker());
            Poll::Pending
        }
    }
}

impl Drop for Sleep {
    /// Remove a pending timer entry when the future is dropped early.
    ///
    /// This prevents stale wakers from remaining in the runtime after the
    /// corresponding task has been cancelled or a timeout race has completed.
    fn drop(&mut self) {
        if let SleepState::Registered { handle, id, .. } = &self.state {
            handle.cancel_timer(*id);
        }
    }
}

/// Wake every task collected from a due-timer scan.
///
/// Waking happens only after the time-state mutex has been released so resumed
/// tasks can inspect or mutate timer state without deadlocking on the same
/// lock.
pub(super) fn wake_all(wakers: Vec<Waker>) {
    for waker in wakers {
        waker.wake();
    }
}
