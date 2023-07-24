use futures::future::{Fuse, FusedFuture};
use futures::stream::FusedStream;
use futures::{FutureExt, Stream};
use pin_project_lite::pin_project;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};

pin_project! {
    pub struct FutureQueue<T, F, Fut> {
        queue: VecDeque<T>,
        f: F,
        #[pin]
        fut: Fuse<Fut>,
    }
}

pub fn future_queue<T, F, Fut>(f: F) -> FutureQueue<T, F, Fut>
where
    F: FnMut(T) -> Fut,
    Fut: Future,
{
    FutureQueue {
        queue: VecDeque::new(),
        f,
        fut: Fuse::terminated(),
    }
}

impl<T, F, Fut> FutureQueue<T, F, Fut>
where
    F: FnMut(T) -> Fut,
    Fut: Future,
{
    pub fn push(self: Pin<&mut Self>, item: T) {
        self.project().queue.push_back(item)
    }
    pub fn push_unpin(&mut self, item: T) {
        self.queue.push_back(item)
    }
}

impl<T, F, Fut> Stream for FutureQueue<T, F, Fut>
where
    F: FnMut(T) -> Fut,
    Fut: Future,
{
    type Item = Fut::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        let mut me = self.project();
        loop {
            if !me.fut.is_terminated() {
                return me.fut.poll(cx).map(Some);
            }
            let Some(item) = me.queue.pop_front() else {
                return Poll::Ready(None);
            };
            let fut = (me.f)(item);
            me.fut.as_mut().set(fut.fuse());
        }
    }
}

impl<T, F, Fut> FusedStream for FutureQueue<T, F, Fut>
where
    F: FnMut(T) -> Fut,
    Fut: Future,
{
    fn is_terminated(&self) -> bool {
        self.fut.is_terminated() && self.queue.is_empty()
    }
}
