use std::{
    fmt::{self, Write as _},
    hash::{Hash, Hasher},
    ops::Deref,
};

use derive_more::{From, Into};
use spacetimedb_data_structures::map::{IntMap, ValidAsIdentityHash};

use crate::{blob_store::BlobHash, page::Page};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageHash(blake3::Hash);

impl PageHash {
    pub const SIZE: usize = blake3::OUT_LEN;

    pub fn as_bytes(&self) -> &[u8; Self::SIZE] {
        self.0.as_bytes()
    }
}

impl Hash for PageHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_prefix(self.as_bytes(), state)
    }
}

impl From<&mut Page> for PageHash {
    fn from(page: &mut Page) -> Self {
        Self(page.save_or_get_content_hash())
    }
}

impl ValidAsIdentityHash for PageHash {}

/// Wrapper around a [crate::blob_store::BlobHash] that allows it to be used as
/// a key in an [IntMap].
///
/// Because the original blob hash type has a derived `Hash` impl and is public,
/// some users may be relying on its hashing semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, From, Into)]
pub struct BlobHashWrapper(BlobHash);

impl BlobHashWrapper {
    pub fn as_bytes(&self) -> &[u8; BlobHash::SIZE] {
        &self.0.data
    }

    pub fn hash_from_bytes(bytes: &[u8]) -> Self {
        Self(BlobHash::hash_from_bytes(bytes))
    }
}

impl Hash for BlobHashWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_prefix(self.as_bytes(), state)
    }
}

impl From<[u8; 32]> for BlobHashWrapper {
    fn from(data: [u8; 32]) -> Self {
        Self(BlobHash { data })
    }
}

impl Deref for BlobHashWrapper {
    type Target = BlobHash;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ValidAsIdentityHash for BlobHashWrapper {}

fn hash_prefix<H: Hasher>(bytes: &[u8], state: &mut H) {
    const PREFIX_LEN: usize = (u64::BITS / 8) as usize;
    state.write_u64(u64::from_le_bytes(bytes[..PREFIX_LEN].try_into().unwrap()));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKey {
    Page(PageHash),
    Blob(BlobHashWrapper),
}

impl ObjectKey {
    pub const SIZE: usize = 32;

    #[inline]
    pub fn as_bytes(&self) -> &[u8; Self::SIZE] {
        match self {
            Self::Page(hash) => hash.as_bytes(),
            Self::Blob(hash) => hash.as_bytes(),
        }
    }

    pub fn as_page_hash(&self) -> Option<&PageHash> {
        match self {
            Self::Page(hash) => Some(hash),
            Self::Blob(_) => None,
        }
    }

    pub fn as_blob_hash(&self) -> Option<&BlobHashWrapper> {
        match self {
            Self::Blob(hash) => Some(hash),
            Self::Page(_) => None,
        }
    }
}

const _: () = {
    assert!(BlobHash::SIZE == PageHash::SIZE);
    assert!(ObjectKey::SIZE == BlobHash::SIZE);
};

impl Hash for ObjectKey {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Page(hash) => hash.hash(state),
            Self::Blob(hash) => hash.hash(state),
        }
    }
}

impl ValidAsIdentityHash for ObjectKey {}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const ALPHABET: &[u8; 16] = b"0123456789abcdef";
        for &b in self.as_bytes().iter() {
            f.write_char(ALPHABET[(b >> 4) as usize] as char)?;
            f.write_char(ALPHABET[(b & 0xf) as usize] as char)?;
        }

        Ok(())
    }
}

impl From<BlobHashWrapper> for ObjectKey {
    fn from(hash: BlobHashWrapper) -> Self {
        Self::Blob(hash)
    }
}

impl From<BlobHash> for ObjectKey {
    fn from(hash: BlobHash) -> Self {
        Self::from(BlobHashWrapper::from(hash))
    }
}

impl From<PageHash> for ObjectKey {
    fn from(hash: PageHash) -> Self {
        Self::Page(hash)
    }
}

impl From<&mut Page> for ObjectKey {
    fn from(page: &mut Page) -> Self {
        Self::Page(page.into())
    }
}

pub type ObjectMap<V> = IntMap<ObjectKey, V>;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{collection::btree_set, prelude::*};

    proptest! {
        #[test]
        fn keys_with_identity_hasher_collisions_remain_distinct(
            prefix: [u8; 8],
            suffixes in btree_set(any::<[u8; 24]>(), 0..100),
        ) {
            let mut objects = ObjectMap::<usize>::default();
            for (value, suffix) in suffixes.iter().enumerate() {
                let mut bytes = [0; 32];
                bytes[..8].copy_from_slice(&prefix);
                bytes[8..].copy_from_slice(suffix);
                prop_assert_eq!(objects.insert(ObjectKey::from(BlobHashWrapper::from(bytes)), value), None);
            }

            prop_assert_eq!(objects.len(), suffixes.len());
            for (value, suffix) in suffixes.iter().enumerate() {
                let mut bytes = [0; 32];
                bytes[..8].copy_from_slice(&prefix);
                bytes[8..].copy_from_slice(suffix);
                prop_assert_eq!(objects.get(&ObjectKey::from(BlobHashWrapper::from(bytes))), Some(&value));
            }
        }

        #[test]
        fn object_key_displays_as_lowercase_hex(bytes: [u8; 32]) {
            let displayed = ObjectKey::Blob(BlobHashWrapper::from(bytes)).to_string();
            let expected = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();

            prop_assert_eq!(displayed.len(), 64);
            prop_assert_eq!(displayed, expected);
        }
    }
}
