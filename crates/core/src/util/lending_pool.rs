//! like, a semaphore but with values. or something

use std::collections::VecDeque;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use parking_lot::Mutex;
use spacetimedb_lib::Address;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::worker_metrics::{MAX_QUEUE_LEN, WORKER_METRICS};

use super::notify_once::{NotifiedOnce, NotifyOnce};

pub struct LendingPool<T> {
    sem: Arc<Semaphore>,
    inner: Arc<LendingPoolInner<T>>,
}

impl<T> Default for LendingPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for LendingPool<T> {
    fn clone(&self) -> Self {
        Self {
            sem: self.sem.clone(),
            inner: self.inner.clone(),
        }
    }
}

struct LendingPoolInner<T> {
    closed_notify: NotifyOnce,
    vec: Mutex<PoolVec<T>>,
}

struct PoolVec<T> {
    total_count: usize,
    deque: Option<VecDeque<T>>,
}

#[derive(Debug)]
pub struct PoolClosed;

impl<T> LendingPool<T> {
    pub fn new() -> Self {
        Self::from_iter(std::iter::empty())
    }

    pub fn request_with_context(&self, db: Address) -> impl Future<Output = Result<LentResource<T>, PoolClosed>> {
        let acq = self.sem.clone().acquire_owned();
        let pool_inner = self.inner.clone();

        let queue_len = WORKER_METRICS.instance_queue_length.with_label_values(&db);
        let queue_len_max = WORKER_METRICS.instance_queue_length_max.with_label_values(&db);
        let queue_len_histogram = WORKER_METRICS.instance_queue_length_histogram.with_label_values(&db);

        queue_len.inc();
        let new_queue_len = queue_len.get();
        queue_len_histogram.observe(new_queue_len as f64);

        let mut guard = MAX_QUEUE_LEN.lock().unwrap();
        let max_queue_len = *guard
            .entry(db)
            .and_modify(|max| {
                if new_queue_len > *max {
                    *max = new_queue_len;
                }
            })
            .or_insert_with(|| new_queue_len);

        drop(guard);
        queue_len_max.set(max_queue_len);

        async move {
            let permit_result = acq.await.map_err(|_| PoolClosed);
            queue_len.dec();
            queue_len_histogram.observe(queue_len.get() as f64);
            let permit = permit_result?;
            let resource = pool_inner
                .vec
                .lock()
                .deque
                .as_mut()
                .ok_or(PoolClosed)?
                .pop_front()
                .ok_or(PoolClosed)?;
            Ok(LentResource {
                resource: ManuallyDrop::new(resource),
                permit: ManuallyDrop::new(permit),
                pool_inner,
            })
        }
    }

    pub fn add(&self, resource: T) -> Result<(), PoolClosed> {
        self.add_multiple(std::iter::once(resource))
    }

    pub fn add_multiple<I: IntoIterator<Item = T>>(&self, resources: I) -> Result<(), PoolClosed> {
        let resources = resources.into_iter();
        let mut inner = self.inner.vec.lock();
        let deque = inner.deque.as_mut().ok_or(PoolClosed)?;
        let mut num_new = 0;
        deque.extend(resources.inspect(|_| num_new += 1));
        inner.total_count += num_new;
        self.sem.add_permits(num_new);
        Ok(())
    }

    pub fn num_total(&self) -> usize {
        self.inner.vec.lock().total_count
    }

    pub fn num_available(&self) -> usize {
        self.sem.available_permits()
    }

    pub fn close(&self) -> Closed<'_> {
        let mut vec = self.inner.vec.lock();
        self.sem.close();
        if let Some(deque) = vec.deque.take() {
            vec.total_count -= deque.len();
        }
        if vec.total_count == 0 {
            self.inner.closed_notify.notify();
        }
        self.closed()
    }

    pub fn closed(&self) -> Closed<'_> {
        Closed {
            notified: self.inner.closed_notify.notified(),
        }
    }
}

impl<T> FromIterator<T> for LendingPool<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let deque = VecDeque::from_iter(iter);
        Self {
            sem: Arc::new(Semaphore::new(deque.len())),
            inner: Arc::new(LendingPoolInner {
                closed_notify: NotifyOnce::new(),
                vec: Mutex::new(PoolVec {
                    total_count: deque.len(),
                    deque: Some(deque),
                }),
            }),
        }
    }
}

pin_project_lite::pin_project! {
    pub struct Closed<'a> {
        #[pin]
        notified: NotifiedOnce<'a>,
    }
}

impl Future for Closed<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().notified.poll(cx)
    }
}

pub struct LentResource<T> {
    resource: ManuallyDrop<T>,
    permit: ManuallyDrop<OwnedSemaphorePermit>,
    pool_inner: Arc<LendingPoolInner<T>>,
}

impl<T> Deref for LentResource<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.resource
    }
}

impl<T> DerefMut for LentResource<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.resource
    }
}

impl<T> Drop for LentResource<T> {
    fn drop(&mut self) {
        let resource = unsafe { ManuallyDrop::take(&mut self.resource) };
        let permit = unsafe { ManuallyDrop::take(&mut self.permit) };
        {
            let mut vec = self.pool_inner.vec.lock();
            if let Some(deque) = &mut vec.deque {
                deque.push_back(resource);
                drop(permit);
            } else {
                drop(resource);
                permit.forget();
                vec.total_count -= 1;
                if vec.total_count == 0 {
                    self.pool_inner.closed_notify.notify();
                }
            }
        }
    }
}
