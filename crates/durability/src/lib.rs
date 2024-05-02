use std::sync::Arc;

pub use spacetimedb_commitlog::{error, payload::Txdata, Decoder, Transaction};

mod imp;
pub use imp::{local, Local};

/// Transaction offset.
///
/// The transaction offset is essentially a monotonic counter of all
/// transactions submitted to the durability layer, starting from zero.
///
/// While the implementation may not guarantee that the sequence contains no
/// gaps, it must guarantee that a higher transaction offset implies durability
/// of all offsets smaller than it.
pub type TxOffset = u64;

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
    fn durable_tx_offset(&self) -> Option<TxOffset>;
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
    /// The default implementation returns `None`, which is correct for any
    /// history implementation.
    fn max_tx_offset(&self) -> Option<TxOffset> {
        None
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

    fn max_tx_offset(&self) -> Option<TxOffset> {
        (**self).max_tx_offset()
    }
}
