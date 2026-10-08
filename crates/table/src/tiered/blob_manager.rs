use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use crate::{
    blob_store::BlobHash,
    tiered::{error::BlobError, map::ObjectMap, BudgetPermit, ByteBudget, TieredStorage},
};

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

    pub(super) fn install_or_get_resident(
        &self,
        hash: BlobHash,
        bytes: &[u8],
        access_epoch: u64,
    ) -> Result<BlobHandle, BlobError> {
        todo!();
    }

    pub fn get(&self, hash: BlobHash) -> Result<BlobHandle, BlobError> {
        if let Some(resident) = self.frames.get(hash) {
            return Ok(resident.into_handle());
        }

        let object_key = hash.into();
        let meta = self.store.stat(object_key)?;
        let permit = self.memory.acquire(meta.size)?;
        let bytes = self.store.read(hash.into())?;
        let frame = self.frames.register(permit, hash, bytes);

        Ok(frame.into_handle())
    }

    /*
    fn ensure_lower_tier_source(&self, frame: &Arc<BlobFrame>) -> Result<(), BlobError> {
        todo!()
    }
    */
}

#[derive(Default)]
pub struct BlobFrameRegistry {
    entries: Mutex<ObjectMap<Arc<BlobFrame>>>,
}

impl BlobFrameRegistry {
    fn get(&self, hash: BlobHash) -> Option<Arc<BlobFrame>> {
        self.entries.lock().unwrap().get(&hash.into()).cloned()
    }

    fn register(&self, permit: BudgetPermit, hash: BlobHash, bytes: Box<[u8]>) -> Arc<BlobFrame> {
        let frame = Arc::new(BlobFrame { hash, bytes, permit });
        let prev = self.entries.lock().unwrap().insert(hash.into(), frame.clone());
        assert!(prev.is_none());

        frame
    }
}

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
