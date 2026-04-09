use std::{cmp::Reverse, collections::BinaryHeap, iter, marker::PhantomData, num::NonZeroUsize, sync::Arc};

use futures::future::BoxFuture;
use thiserror::Error;
use tokio::sync::watch;

pub use spacetimedb_commitlog::{error, payload::Txdata, Decoder, Transaction};

mod imp;
pub use imp::*;

/// Transaction offset.
///
/// The transaction offset is essentially a monotonic counter of all
/// transactions submitted to the durability layer, starting from zero.
///
/// While the implementation may not guarantee that the sequence contains no
/// gaps, it must guarantee that a higher transaction offset implies durability
/// of all offsets smaller than it.
pub type TxOffset = u64;

#[derive(Debug, Error)]
pub enum ReorderError {
    #[error("reordering window exceeded")]
    SizeExceeded,
    #[error("transaction offset behind expected offset")]
    TxBehind,
}

/// A bounded collection of elements ordered by [TxOffset], backed by a [BinaryHeap].
///
/// This exists to tolerate slightly out-of-order transaction requests while
/// still preserving contiguous commit order.
pub struct ReorderWindow<T> {
    heap: BinaryHeap<Reverse<TxOrdered<T>>>,
    next_tx: TxOffset,
    max_len: NonZeroUsize,
}

impl<T> ReorderWindow<T> {
    pub fn new(next_tx: TxOffset, max_len: NonZeroUsize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(1),
            next_tx,
            max_len,
        }
    }

    pub fn push(&mut self, tx_offset: TxOffset, inner: T) -> Result<(), ReorderError> {
        if self.len() >= self.max_len.get() {
            return Err(ReorderError::SizeExceeded);
        }
        self.push_maybe_overfull(TxOrdered { tx_offset, inner })
    }

    pub fn push_batch_ready(&mut self, items: impl IntoIterator<Item = (TxOffset, T)>) -> Result<Vec<T>, ReorderError> {
        let mut ready = Vec::new();
        for (tx_offset, inner) in items {
            // A drained batch may include both the missing next offset and later offsets
            // queued behind it. Fail only if the expected offset is still missing and the
            // reorder window exceeds capacity.
            self.push_maybe_overfull(TxOrdered { tx_offset, inner })?;
            ready.extend(self.drain());
            if self.len() > self.max_len.get() {
                return Err(ReorderError::SizeExceeded);
            }
        }
        Ok(ready)
    }

    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        iter::from_fn(|| {
            let min_tx_offset = self.heap.peek().map(|Reverse(item)| item.tx_offset);
            if min_tx_offset.is_some_and(|tx_offset| tx_offset == self.next_tx) {
                let Reverse(TxOrdered { inner, .. }) = self.heap.pop().unwrap();
                self.next_tx += 1;
                Some(inner)
            } else {
                None
            }
        })
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    fn push_maybe_overfull(&mut self, item: TxOrdered<T>) -> Result<(), ReorderError> {
        if item.tx_offset < self.next_tx {
            return Err(ReorderError::TxBehind);
        }
        if !self.heap.is_empty() {
            self.heap.reserve_exact(self.max_len.get());
        }
        self.heap.push(Reverse(item));
        Ok(())
    }
}

struct TxOrdered<T> {
    tx_offset: TxOffset,
    inner: T,
}

impl<T> PartialEq for TxOrdered<T> {
    fn eq(&self, other: &Self) -> bool {
        self.tx_offset == other.tx_offset
    }
}

impl<T> Eq for TxOrdered<T> {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl<T> PartialOrd for TxOrdered<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.tx_offset.cmp(&other.tx_offset))
    }
}

impl<T> Ord for TxOrdered<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug, Error)]
#[error("the database's durability layer went away")]
pub struct DurabilityExited;

/// Handle to the durable offset, obtained via [`Durability::durable_tx_offset`].
///
/// The handle can be used to read the current durable offset, or wait for a
/// provided offset to be reached.
///
/// The handle is valid for as long as the [`Durability`] instance it was
/// obtained from is live, i.e. able to persist transactions. When the instance
/// shuts down or crashes, methods will return errors of type [`DurabilityExited`].
#[derive(Clone)]
pub struct DurableOffset {
    // TODO: `watch::Receiver::wait_for` will hold a shared lock until all
    // subscribers have seen the current value. Although it may skip entries,
    // this may cause unacceptable contention. We may consider a custom watch
    // channel that operates on an `AtomicU64` instead of an `RwLock`.
    inner: watch::Receiver<Option<TxOffset>>,
}

impl DurableOffset {
    /// Get the current durable offset, or `None` if no transaction has been
    /// made durable yet.
    ///
    /// Returns `Err` if the associated durablity is no longer live.
    pub fn get(&self) -> Result<Option<TxOffset>, DurabilityExited> {
        self.guard_closed().map(|()| self.inner.borrow().as_ref().copied())
    }

    /// Get the current durable offset, even if the associated durability is
    /// no longer live.
    pub fn last_seen(&self) -> Option<TxOffset> {
        self.inner.borrow().as_ref().copied()
    }

    /// Wait for `offset` to become durable, i.e.
    ///
    /// ```ignore
    ///     self.get().unwrap().is_some_and(|durable| durable >= offset)
    /// ```
    ///
    /// Returns the actual durable offset at which above condition evaluated to
    /// `true`, or an `Err` if the durability is no longer live.
    ///
    /// Returns immediately if the condition evaluates to `true` for the current
    /// durable offset.
    pub async fn wait_for(&mut self, offset: TxOffset) -> Result<TxOffset, DurabilityExited> {
        self.inner
            .wait_for(|durable| durable.is_some_and(|val| val >= offset))
            .await
            .map(|r| r.as_ref().copied().unwrap())
            .map_err(|_| DurabilityExited)
    }

    fn guard_closed(&self) -> Result<(), DurabilityExited> {
        self.inner.has_changed().map(drop).map_err(|_| DurabilityExited)
    }
}

impl From<watch::Receiver<Option<TxOffset>>> for DurableOffset {
    fn from(inner: watch::Receiver<Option<TxOffset>>) -> Self {
        Self { inner }
    }
}

/// Future created by [Durability::close].
///
/// This is a boxed future rather than an associated type, so that [Durability]
/// can be used as a trait object without knowing the type of the `close` future.
pub type Close = BoxFuture<'static, Option<TxOffset>>;

/// The durability API.
///
/// NOTE: This is a preliminary definition, still under consideration.
///
/// A durability implementation accepts a [Transaction] to be made durable via
/// the [Durability::append_tx] method in a non-blocking fashion.
///
/// Once a transaction becomes durable, the [DurableOffset] is updated.
/// What durable means depends on the implementation, informally it can be
/// thought of as "written to disk".
pub trait Durability: Send + Sync {
    /// The payload representing a single transaction.
    type TxData;

    /// Submit a [Transaction] to be made durable.
    ///
    /// This method must never block, and accept new transactions even if they
    /// cannot be made durable immediately.
    ///
    /// Errors may be signalled by panicking.
    //
    // TODO: Support batches of txs, i.e. commits.
    //
    // The commitlog supports this, but allocation overhead in the durability
    // API is too high given we don't make any use of it.
    //
    // We don't make any use of it because a commit is an atomic unit of storage
    // (i.e. a torn write will corrupt all transactions contained in it), and it
    // is very unclear when it is both correct and beneficial to bundle more
    // than a single transaction into a commit.
    fn append_tx(&self, tx: Transaction<Self::TxData>);

    /// Obtain a handle to the [DurableOffset].
    fn durable_tx_offset(&self) -> DurableOffset;

    /// Asynchronously request the durability to shut down, without dropping it.
    ///
    /// Shall close any internal channels, such that it is no longer possible to
    /// append new data (i.e. [Durability::append_tx] shall panic).
    /// Then, drains the internal queues and attempts to make the remaining data
    /// durable. Resolves to the durable [TxOffset].
    ///
    /// When the returned future resolves, calls to [Durability::append_tx] must
    /// panic, and calling [DurableOffset::last_seen] must return the same value
    /// as the future's output.
    ///
    /// Repeatedly calling `close` on an already closed [Durability] shall
    /// return the same [TxOffset].
    ///
    /// Note that errors are not propagated, as the [Durability] may already be
    /// closed.
    ///
    /// # Cancellation
    ///
    /// Dropping the [Close] future shall abort the shutdown process,
    /// and leave the [Durability] in a closed state.
    fn close(&self) -> Close;
}

/// Access to the durable history.
///
/// The durable history is the sequence of transactions in the order
/// [`Durability::append_tx`] was called.
///
/// Some [`Durability`] implementations will be able to also implement this
/// trait, but others may not. A database may also use a [`Durability`]
/// implementation to persist transactions, but a separate [`History`]
/// implementation to obtain the history.
pub trait History {
    type TxData;

    /// Traverse the history of transactions from `offset` and "fold" it into
    /// the provided [`Decoder`].
    fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>;

    /// Obtain an iterator over the history of transactions, starting from `offset`.
    fn transactions_from<'a, D>(
        &self,
        offset: TxOffset,
        decoder: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<Self::TxData>, D::Error>>
    where
        D: Decoder<Record = Self::TxData>,
        D::Error: From<error::Traversal>,
        Self::TxData: 'a;

    /// Get the maximum transaction offset contained in this history.
    ///
    /// Similar to [`std::iter::Iterator::size_hint`], this is considered an
    /// estimation: the upper bound may not be known, or it may change after
    /// this method was called because more data was added to the log.
    ///
    /// Callers should thus only rely on it for informational purposes.
    ///
    /// The default implementation returns `(0, None)`, which is correct for any
    /// history implementation.
    fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
        (0, None)
    }
}

impl<T: History> History for Arc<T> {
    type TxData = T::TxData;

    fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        (**self).fold_transactions_from(offset, decoder)
    }

    fn transactions_from<'a, D>(
        &self,
        offset: TxOffset,
        decoder: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<Self::TxData>, D::Error>>
    where
        D: Decoder<Record = Self::TxData>,
        D::Error: From<error::Traversal>,
        Self::TxData: 'a,
    {
        (**self).transactions_from(offset, decoder)
    }

    fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
        (**self).tx_range_hint()
    }
}

#[derive(Default)]
pub struct EmptyHistory<T> {
    _txdata: PhantomData<T>,
}

impl<T> EmptyHistory<T> {
    pub const fn new() -> Self {
        Self { _txdata: PhantomData }
    }
}

impl<T> History for EmptyHistory<T> {
    type TxData = T;

    fn fold_transactions_from<D>(&self, _offset: TxOffset, _decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        Ok(())
    }

    fn transactions_from<'a, D>(
        &self,
        _offset: TxOffset,
        _decoder: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<Self::TxData>, D::Error>>
    where
        D: Decoder<Record = Self::TxData>,
        D::Error: From<error::Traversal>,
        Self::TxData: 'a,
    {
        iter::empty()
    }

    fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
        (0, Some(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_window_sorts_by_tx_offset() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(5).unwrap());

        for tx_offset in (0..5).rev() {
            win.push(tx_offset, tx_offset).unwrap();
        }

        let txs = win.drain().collect::<Vec<_>>();
        assert_eq!(txs, &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn reorder_window_stops_drain_at_gap() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(5).unwrap());

        win.push(4, 4).unwrap();
        assert!(win.drain().collect::<Vec<_>>().is_empty());

        for tx_offset in 0..4 {
            win.push(tx_offset, tx_offset).unwrap();
        }

        let txs = win.drain().collect::<Vec<_>>();
        assert_eq!(&txs, &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn reorder_window_error_when_full() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(1).unwrap());
        win.push(0, ()).unwrap();
        assert!(matches!(win.push(1, ()), Err(ReorderError::SizeExceeded)));
    }

    #[test]
    fn reorder_window_error_on_late_request() {
        let mut win = ReorderWindow::new(1, NonZeroUsize::new(5).unwrap());
        assert!(matches!(win.push(0, ()), Err(ReorderError::TxBehind)));
    }

    #[test]
    fn reorder_window_allows_batch_to_close_gap() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(8).unwrap());

        let ready = win
            .push_batch_ready((0..=8).rev().map(|tx_offset| (tx_offset, tx_offset)))
            .unwrap();

        assert_eq!(ready, (0..=8).collect::<Vec<_>>());
    }

    #[test]
    fn reorder_window_errors_when_gap_exceeds_capacity() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(8).unwrap());

        let err = win
            .push_batch_ready((1..=9).map(|tx_offset| (tx_offset, tx_offset)))
            .unwrap_err();

        assert!(matches!(err, ReorderError::SizeExceeded));
    }
}
