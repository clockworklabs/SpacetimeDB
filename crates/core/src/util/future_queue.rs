use futures::future::MaybeDone;
use futures::stream::FusedStream;
use futures::Stream;
use pin_project_lite::pin_project;
use std::collections::VecDeque;
use std::future::{poll_fn, Future};
use std::pin::Pin;
use std::task::{self, Poll};

pin_project! {
    /// A FIFO queue into which `Job`s can be pushed, which maintains at most one running `Fut` at a time.
    ///
    /// Each subscribed/connected WebSocket maintains a `FutureQueue` of incoming messages to handle.
    ///
    /// `Fut` should implement `Future`.
    /// `StartFn` should implement `FnMut(Job) -> Fut`.
    pub struct FutureQueue<Job, StartFn, Fut: Future> {
        job_queue: VecDeque<Job>,
        start_fn: StartFn,
        #[pin]
        running_job: MaybeDone<Fut>,
    }
}

/// Construct a `FutureQueue` which uses `start_fn` to run its frontmost job.
pub fn future_queue<Job, StartFn, Fut>(start_fn: StartFn) -> FutureQueue<Job, StartFn, Fut>
where
    StartFn: FnMut(Job) -> Fut,
    Fut: Future,
{
    FutureQueue {
        job_queue: VecDeque::new(),
        start_fn,
        running_job: MaybeDone::Gone,
    }
}

impl<Job, StartFn, Fut> FutureQueue<Job, StartFn, Fut>
where
    StartFn: FnMut(Job) -> Fut,
    Fut: Future,
{
    /// Insert a job into the FIFO queue.
    ///
    /// When the job reaches the front of the queue and this queue is awaited,
    /// `self.start_fn` will be applied to `job` to start it,
    /// and awaiting this queue will await that future.
    ///
    /// As with all futures, the job will not run unless awaited.
    /// In addition, `FutureQueue` will not start a new job until the previous job has finished,
    /// so `self.start_fn` will not be called until `self` is polled
    /// enough times to consume all earlier entries in the queue.
    pub fn push(self: Pin<&mut Self>, job: Job) {
        self.project().job_queue.push_back(job)
    }

    /// Insert a job into the FIFO queue.
    ///
    /// When the job reaches the front of the queue and this queue is awaited,
    /// `self.start_fn` will be applied to `job` to start it,
    /// and awaiting this queue will await that future.
    ///
    /// As with all futures, the job will not run unless awaited.
    /// In addition, `FutureQueue` will not start a new job until the previous job has finished,
    /// so `self.start_fn` will not be called until `self` is polled
    /// enough times to consume all earlier entries in the queue.
    pub fn push_unpin(&mut self, job: Job) {
        self.job_queue.push_back(job)
    }

    /// Remove all jobs from the queue without running them, and cancel the current job if one is running.
    ///
    /// Subscriptions clear their queue upon disconnecting,
    /// to avoid leaving stale jobs that will never be started or awaited.
    pub fn clear(self: Pin<&mut Self>) {
        let mut me = self.project();
        me.job_queue.clear();
        me.running_job.set(MaybeDone::Gone);
    }

    pub fn queue_len(&self) -> usize {
        self.job_queue.len()
    }

    pub async fn make_progress(self: &mut Pin<&mut Self>) {
        poll_fn(|cx| self.as_mut().poll_progress(cx)).await
    }

    pub fn poll_progress(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<()> {
        let me = self.project();
        if let MaybeDone::Gone = &*me.running_job {
            Poll::Ready(())
        } else {
            me.running_job.poll(cx)
        }
    }
}

impl<Job, StartFn, Fut> Stream for FutureQueue<Job, StartFn, Fut>
where
    StartFn: FnMut(Job) -> Fut,
    Fut: Future,
{
    type Item = Fut::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        let mut me = self.project();

        if let MaybeDone::Gone = &*me.running_job {
            let Some(item) = me.job_queue.pop_front() else {
                return Poll::Ready(None);
            };
            let fut = (me.start_fn)(item);
            me.running_job.as_mut().set(MaybeDone::Future(fut));
        }

        me.running_job.as_mut().poll(cx).map(|()| me.running_job.take_output())
    }
}

impl<Job, StartFn, Fut> FusedStream for FutureQueue<Job, StartFn, Fut>
where
    StartFn: FnMut(Job) -> Fut,
    Fut: Future,
{
    fn is_terminated(&self) -> bool {
        matches!(self.running_job, MaybeDone::Gone) && self.job_queue.is_empty()
    }
}
