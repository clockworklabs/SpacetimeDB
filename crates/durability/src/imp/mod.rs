pub mod local;
pub use local::Local;

#[cfg(any(test, feature = "test"))]
pub use testing::NoDurability;

#[cfg(any(test, feature = "test"))]
mod testing {
    use std::{
        future,
        marker::PhantomData,
        sync::atomic::{AtomicBool, Ordering},
    };

    use futures::FutureExt as _;
    use tokio::sync::watch;

    use crate::{Close, Durability, DurableOffset, TxOffset};

    /// A [`Durability`] impl that sends all transactions into the void.
    ///
    /// This should only be used for testing, and is thus only available when
    /// the `test` feature is enabled.
    pub struct NoDurability<T> {
        durable_offset: watch::Sender<Option<TxOffset>>,
        closed: AtomicBool,
        _txdata: PhantomData<T>,
    }

    impl<T> Default for NoDurability<T> {
        fn default() -> Self {
            let (durable_offset, _) = watch::channel(None);
            Self {
                durable_offset,
                closed: AtomicBool::new(false),
                _txdata: PhantomData,
            }
        }
    }

    impl<T: Send + Sync> Durability for NoDurability<T> {
        type TxData = T;

        fn append_tx(&self, _: Self::TxData) {
            if self.closed.load(Ordering::Relaxed) {
                panic!("`close` was called on this `NoDurability` instance");
            }
        }

        fn durable_tx_offset(&self) -> DurableOffset {
            self.durable_offset.subscribe().into()
        }

        fn close(&self) -> Close {
            self.closed.store(true, Ordering::Relaxed);
            future::ready(*self.durable_offset.borrow()).boxed()
        }
    }
}
