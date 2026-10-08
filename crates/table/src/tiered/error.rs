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
    MemoryLimitExceeded(#[from] BudgetExceeded),
    #[error(transparent)]
    Page(page::Error),
    #[error("page at index {0:?} is missing")]
    MissingPage(PageIndex),
    #[error("object {0} is missing")]
    MissingObject(blake3::Hash),
    #[error("error decoding page from bsatn")]
    Deserialize(#[from] DecodeError),
    #[error(transparent)]
    Storage(#[from] ObjectStoreError),
}

impl PageError {
    pub fn is_transient(&self) -> bool {
        match self {
            PageError::TooManyPages
            | PageError::MemoryLimitExceeded(_)
            | PageError::Page(_)
            | PageError::MissingPage(_)
            | PageError::MissingObject(_)
            | PageError::Deserialize(_) => false,

            PageError::Storage(storage) => match storage {
                ObjectStoreError::UnknownObject(_)
                | ObjectStoreError::MissingObject(_)
                | ObjectStoreError::HashMismatch { .. } => false,

                ObjectStoreError::Io(_) => true,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error(transparent)]
    MemoryLimitExceeded(#[from] BudgetExceeded),
    #[error("blob {0} is missing")]
    MissingBlob(BlobHash),
    #[error(transparent)]
    Storage(#[from] ObjectStoreError),
}

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
    Io(#[from] io::Error),
}
