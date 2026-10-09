use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use crate::{
    blob_store::BlobHash,
    tiered::{error::BlobError, map::ObjectMap, BudgetPermit, ByteBudget, TieredStorage},
};

#[derive(Debug)]
pub struct BlobManager {
    frames: Arc<BlobFrameRegistry>,
    store: Arc<TieredStorage>,
    memory: ByteBudget,
}

impl BlobManager {
    pub fn new_for_test() -> Self {
        Self {
            frames: <_>::default(),
            store: TieredStorage::new_for_test().into(),
            memory: ByteBudget::unlimited(),
        }
    }

    pub(super) fn get_or_install(
        &self,
        hash: BlobHash,
        bytes: &[u8],
        _access_epoch: u64,
    ) -> Result<BlobHandle, BlobError> {
        self.frames
            .get_or_try_register(hash, || {
                self.memory
                    .acquire(bytes.len() as u64)
                    .map(|permit| (permit, bytes.into()))
            })
            .map_err(Into::into)
            .map(|frame| frame.into_handle())
    }

    pub fn get_or_fault(&self, hash: BlobHash) -> Result<BlobHandle, BlobError> {
        self.frames
            .get_or_try_register(hash, || {
                let size = self.store.stat(hash.into())?.size;
                let permit = self.memory.acquire(size)?;
                let bytes = self.store.read(hash.into())?;

                Ok((permit, bytes))
            })
            .map(|frame| frame.into_handle())
    }

    /*
    fn ensure_lower_tier_source(&self, frame: &Arc<BlobFrame>) -> Result<(), BlobError> {
        todo!()
    }
    */
}

#[derive(Debug, Default)]
pub struct BlobFrameRegistry {
    entries: Mutex<ObjectMap<Arc<BlobFrame>>>,
}

impl BlobFrameRegistry {
    fn get(&self, hash: BlobHash) -> Option<Arc<BlobFrame>> {
        self.entries.lock().unwrap().get(&hash.into()).cloned()
    }

    fn get_or_try_register<E>(
        &self,
        hash: BlobHash,
        f: impl FnOnce() -> Result<(BudgetPermit, Box<[u8]>), E>,
    ) -> Result<Arc<BlobFrame>, E> {
        let mut entries = self.entries.lock().unwrap();
        entries
            .entry(hash.into())
            .or_try_insert_with(|| f().map(|(permit, bytes)| BlobFrame { hash, bytes, permit }.into()))
            .cloned()
    }
}

#[derive(Debug)]
pub struct BlobFrame {
    hash: BlobHash,
    bytes: Box<[u8]>,
    permit: BudgetPermit,
    /*
    last_access_epoch: AtomicU64,
    previous_access_epoch: AtomicU64,
    */
}

impl BlobFrame {
    pub fn into_handle(self: Arc<Self>) -> BlobHandle {
        self.into()
    }
}

#[derive(Clone)]
pub struct BlobHandle {
    frame: Arc<BlobFrame>,
}

#[cfg(test)]
impl BlobHandle {
    pub fn new_for_test(hash: BlobHash, bytes: &[u8], permit: BudgetPermit) -> Self {
        debug_assert_eq!(hash, BlobHash::hash_from_bytes(bytes));

        Self {
            frame: BlobFrame {
                hash,
                bytes: bytes.into(),
                permit,
            }
            .into(),
        }
    }
}

impl Deref for BlobHandle {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.frame.bytes
    }
}

impl From<Arc<BlobFrame>> for BlobHandle {
    fn from(frame: Arc<BlobFrame>) -> Self {
        Self { frame }
    }
}
