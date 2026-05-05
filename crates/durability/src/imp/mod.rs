pub mod local;
pub use local::Local;

#[cfg(any(test, feature = "test"))]
pub use testing::{DirectLocal, NoDurability};

#[cfg(any(test, feature = "test"))]
mod testing {
    use std::{
        future,
        marker::PhantomData,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
    };

    use futures::FutureExt as _;
    use spacetimedb_commitlog::{
        payload::Txdata,
        repo::{Repo, RepoWithoutLockFile},
        Commitlog, Encode,
    };
    use tokio::sync::watch;

    use crate::{local, Close, Durability, DurableOffset, History, PreparedTx, TxOffset};

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

        fn append_tx(&self, _: PreparedTx<Self::TxData>) {
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

    /// A commitlog-backed durability implementation that performs writes inline.
    ///
    /// This is intended for deterministic tests that want to inject their own
    /// execution model instead of using [`local::Local`]'s Tokio actor.
    pub struct DirectLocal<T, R>
    where
        R: Repo,
    {
        clog: Arc<Commitlog<Txdata<T>, R>>,
        durable_offset: watch::Sender<Option<TxOffset>>,
        closed: AtomicBool,
        write_lock: Mutex<()>,
    }

    impl<T, R> DirectLocal<T, R>
    where
        T: Encode + Send + Sync + 'static,
        R: RepoWithoutLockFile + Send + Sync + 'static,
    {
        pub fn open_with_repo(repo: R, opts: local::Options) -> Result<Self, local::OpenError> {
            let clog = Arc::new(Commitlog::open_with_repo(repo, opts.commitlog)?);
            let (durable_offset, _) = watch::channel(clog.max_committed_offset());
            Ok(Self {
                clog,
                durable_offset,
                closed: AtomicBool::new(false),
                write_lock: Mutex::new(()),
            })
        }

        pub fn as_history(&self) -> impl History<TxData = Txdata<T>> + use<T, R> {
            self.clog.clone()
        }
    }

    impl<T, R> DirectLocal<T, R>
    where
        T: Encode + Send + Sync + 'static,
        R: Repo + Send + Sync + 'static,
    {
        fn flush_and_publish(&self) -> Option<TxOffset> {
            let offset = self
                .clog
                .flush_and_sync()
                .expect("direct local durability: commitlog flush-and-sync failed");
            if let Some(offset) = offset {
                self.durable_offset.send_modify(|val| {
                    val.replace(offset);
                });
            }
            self.durable_offset.borrow().as_ref().copied()
        }
    }

    impl<T, R> Durability for DirectLocal<T, R>
    where
        T: Encode + Send + Sync + 'static,
        R: Repo + Send + Sync + 'static,
    {
        type TxData = Txdata<T>;

        fn append_tx(&self, tx: PreparedTx<Self::TxData>) {
            if self.closed.load(Ordering::Relaxed) {
                panic!("`close` was called on this `DirectLocal` instance");
            }
            let _guard = self.write_lock.lock().expect("direct local durability lock poisoned");
            self.clog
                .commit([tx.into_transaction()])
                .expect("direct local durability: commitlog write failed");
            self.flush_and_publish();
        }

        fn durable_tx_offset(&self) -> DurableOffset {
            self.durable_offset.subscribe().into()
        }

        fn close(&self) -> Close {
            self.closed.store(true, Ordering::Relaxed);
            let _guard = self.write_lock.lock().expect("direct local durability lock poisoned");
            future::ready(self.flush_and_publish()).boxed()
        }
    }

    #[cfg(test)]
    mod tests {
        use futures::FutureExt as _;
        use spacetimedb_commitlog::repo::Memory;
        use spacetimedb_sats::ProductValue;

        use super::*;
        use crate::{Durability, Transaction};

        #[test]
        fn direct_local_publishes_durable_offset_inline() {
            let durability = DirectLocal::<ProductValue, Memory>::open_with_repo(
                Memory::new(1024 * 1024),
                local::Options::default(),
            )
            .unwrap();

            durability.append_tx(Box::new(Transaction {
                offset: 0,
                txdata: Txdata {
                    inputs: None,
                    outputs: None,
                    mutations: None,
                },
            }));

            assert_eq!(durability.durable_tx_offset().last_seen(), Some(0));
            assert_eq!(durability.close().now_or_never().flatten(), Some(0));
        }
    }
}
