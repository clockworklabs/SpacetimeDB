use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::task::{ready, Context, Poll};
use tokio::sync::{futures::Notified, Notify};

pub struct NotifyOnce {
    notify: Notify,
    flag: AtomicBool,
}

impl NotifyOnce {
    pub const fn new() -> Self {
        Self {
            notify: Notify::const_new(),
            flag: AtomicBool::new(false),
        }
    }

    // returns true if this is the first time notify() has been called
    pub fn notify(&self) -> bool {
        let prev = self.flag.swap(true, SeqCst);
        self.notify.notify_waiters();
        !prev
    }

    pub fn notified(&self) -> NotifiedOnce<'_> {
        NotifiedOnce {
            notified: self.notify.notified(),
            flag: &self.flag,
        }
    }
}

impl Default for NotifyOnce {
    fn default() -> Self {
        Self::new()
    }
}

pin_project_lite::pin_project! {
    pub struct NotifiedOnce<'a> {
        #[pin]
        notified: Notified<'a>,
        flag: &'a AtomicBool,
    }
}

impl Future for NotifiedOnce<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut me = self.project();
        while !me.flag.load(SeqCst) {
            ready!(me.notified.as_mut().poll(cx))
        }
        Poll::Ready(())
    }
}
