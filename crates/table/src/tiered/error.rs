use std::io;

use spacetimedb_lib::bsatn::DecodeError;

use crate::{
    blob_store::BlobHash,
    indexes::PageIndex,
    page,
    tiered::{map::ObjectKey, BudgetExceeded},
};

#[derive(Debug, thiserror::Error)]
pub enum PageError {
    #[error("maximum number of pages exceeded")]
    TooManyPages,
    #[error(transparent)]
    MemoryLimitExceeded(#[from] Box<BudgetExceeded>),
    #[error(transparent)]
    Page(page::Error),
    #[error("page at index {0:?} is missing")]
    MissingPage(PageIndex),
    #[error("object {0} is missing")]
    MissingObject(Box<blake3::Hash>),
    #[error("error decoding page from bsatn")]
    Deserialize(#[from] Box<DecodeError>),
    #[error(transparent)]
    Storage(#[from] Box<ObjectStoreError>),
}

impl From<BudgetExceeded> for PageError {
    fn from(error: BudgetExceeded) -> Self {
        Self::MemoryLimitExceeded(Box::new(error))
    }
}

impl From<DecodeError> for PageError {
    fn from(error: DecodeError) -> Self {
        Self::Deserialize(Box::new(error))
    }
}

impl From<ObjectStoreError> for PageError {
    fn from(error: ObjectStoreError) -> Self {
        Self::Storage(Box::new(error))
    }
}

impl PageError {
    pub fn is_transient(&self) -> bool {
        match self {
            PageError::MemoryLimitExceeded(_) => true,

            PageError::TooManyPages
            | PageError::Page(_)
            | PageError::MissingPage(_)
            | PageError::MissingObject(_)
            | PageError::Deserialize(_) => false,

            PageError::Storage(storage) => storage.is_transient(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error(transparent)]
    MemoryLimitExceeded(#[from] Box<BudgetExceeded>),
    #[error("blob {0} is missing")]
    MissingBlob(Box<BlobHash>),
    #[error(transparent)]
    Storage(#[from] Box<ObjectStoreError>),
}

impl From<BudgetExceeded> for BlobError {
    fn from(error: BudgetExceeded) -> Self {
        Self::MemoryLimitExceeded(Box::new(error))
    }
}

impl From<ObjectStoreError> for BlobError {
    fn from(error: ObjectStoreError) -> Self {
        Self::Storage(Box::new(error))
    }
}

impl BlobError {
    pub fn is_transient(&self) -> bool {
        match self {
            BlobError::MemoryLimitExceeded(_) | BlobError::MissingBlob(_) => false,
            BlobError::Storage(storage) => storage.is_transient(),
        }
    }
}

// Fault details are cold; keep successful page and row Results small.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<PageError>() == 16);
    assert!(std::mem::size_of::<BlobError>() == 16);
};

#[derive(Debug, thiserror::Error)]
pub enum ObjectStoreError {
    #[error("object {0} is unknown")]
    UnknownObject(ObjectKey),
    #[error("object {0} is missing")]
    MissingObject(ObjectKey),
    #[error("expected object hash {expected} doesn't match computed hash {computed}")]
    HashMismatch {
        expected: blake3::Hash,
        computed: blake3::Hash,
    },
    #[error(transparent)]
    StorageLimitExceeded(#[from] Box<BudgetExceeded>),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl ObjectStoreError {
    pub fn is_transient(&self) -> bool {
        match self {
            ObjectStoreError::UnknownObject(_)
            | ObjectStoreError::MissingObject(_)
            | ObjectStoreError::HashMismatch { .. } => false,

            ObjectStoreError::StorageLimitExceeded(_) | ObjectStoreError::Io(_) => true,
        }
    }
}
