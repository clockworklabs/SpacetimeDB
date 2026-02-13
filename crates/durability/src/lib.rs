use std::{iter, marker::PhantomData, sync::Arc};

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
/// A durability implementation accepts a payload representing a single database
/// transaction via [`Durability::append_tx`] in a non-blocking fashion. The
/// payload _should_ become durable eventually. [`TxOffset`]s reported by
/// [`Durability::durable_tx_offset`] shall be considered durable to the
/// extent the implementation can guarantee.
pub trait Durability: Send + Sync {
    /// The payload representing a single transaction.
    type TxData;

    /// Submit the transaction payload to be made durable.
    ///
    /// This method must never block, and accept new transactions even if they
    /// cannot be made durable immediately.
    ///
    /// A permanent failure of the durable storage may be signalled by panicking.
    fn append_tx(&self, tx: Self::TxData);

    /// The [`TxOffset`] considered durable.
    ///
    /// A `None` return value indicates that the durable offset is not known,
    /// either because nothing has been persisted yet, or because the status
    /// cannot be retrieved.
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
