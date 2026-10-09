use std::{
    fmt::{self, Write as _},
    hash::{BuildHasherDefault, Hash, Hasher},
};

use hashbrown::{
    hash_map::{self, OccupiedEntry, VacantEntry},
    HashMap,
};

use crate::{blob_store::BlobHash, page::Page};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectKey([u8; 32]);

impl ObjectKey {
    fn hash_prefix<const N: usize>(&self) -> &[u8] {
        &self.0[..N]
    }
}

impl Hash for ObjectKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        const PREFIX_LEN: usize = (u64::BITS / 8) as usize;
        let prefix = u64::from_le_bytes(self.hash_prefix::<PREFIX_LEN>().try_into().unwrap());
        state.write_u64(prefix);
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const ALPHABET: &[u8; 16] = b"0123456789abcdef";
        for &b in self.0.iter() {
            f.write_char(ALPHABET[(b >> 4) as usize] as char)?;
            f.write_char(ALPHABET[(b & 0xf) as usize] as char)?;
        }

        Ok(())
    }
}

impl From<BlobHash> for ObjectKey {
    fn from(blob_hash: BlobHash) -> Self {
        Self(blob_hash.data)
    }
}

impl From<ObjectKey> for BlobHash {
    fn from(key: ObjectKey) -> Self {
        BlobHash { data: key.0 }
    }
}

impl From<blake3::Hash> for ObjectKey {
    fn from(hash: blake3::Hash) -> Self {
        Self(*hash.as_bytes())
    }
}

impl From<&mut Page> for ObjectKey {
    fn from(page: &mut Page) -> Self {
        Self::from(page.save_or_get_content_hash())
    }
}

#[derive(Default)]
pub struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0 = u64::from_ne_bytes(bytes.try_into().unwrap());
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}

#[derive(Debug, PartialEq)]
pub struct ObjectMap<V> {
    inner: HashMap<ObjectKey, V, BuildHasherDefault<IdentityHasher>>,
}

impl<V> ObjectMap<V> {
    pub fn contains_key(&self, k: &ObjectKey) -> bool {
        self.inner.contains_key(k)
    }

    pub fn entry(&mut self, k: ObjectKey) -> Entry<'_, V> {
        self.inner.entry(k).into()
    }

    pub fn get(&self, k: &ObjectKey) -> Option<&V> {
        self.inner.get(k)
    }

    pub fn insert(&mut self, k: ObjectKey, v: V) -> Option<V> {
        self.inner.insert(k, v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ObjectKey, &V)> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<V> Default for ObjectMap<V> {
    fn default() -> Self {
        Self { inner: <_>::default() }
    }
}

pub enum Entry<'a, V> {
    Vacant(VacantEntry<'a, ObjectKey, V, BuildHasherDefault<IdentityHasher>>),
    Occupied(OccupiedEntry<'a, ObjectKey, V, BuildHasherDefault<IdentityHasher>>),
}

impl<'a, V> Entry<'a, V> {
    pub fn and_modify(self, f: impl FnOnce(&mut V)) -> Self {
        match self {
            Self::Occupied(mut entry) => {
                f(entry.get_mut());
                Self::Occupied(entry)
            }
            Self::Vacant(entry) => Self::Vacant(entry),
        }
    }

    pub fn or_error<E>(self, f: impl FnOnce() -> E) -> Result<Self, E> {
        if matches!(self, Self::Occupied(_)) {
            return Ok(self);
        }

        Err(f())
    }

    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> &'a mut V {
        match self {
            Self::Vacant(entry) => entry.insert(f()),
            Self::Occupied(entry) => entry.into_mut(),
        }
    }

    pub fn or_try_insert_with<E>(self, f: impl FnOnce() -> Result<V, E>) -> Result<&'a mut V, E> {
        match self {
            Self::Vacant(entry) => f().map(|v| entry.insert(v)),
            Self::Occupied(entry) => Ok(entry.into_mut()),
        }
    }
}

impl<'a, V> From<hash_map::Entry<'a, ObjectKey, V, BuildHasherDefault<IdentityHasher>>> for Entry<'a, V> {
    fn from(entry: hash_map::Entry<'a, ObjectKey, V, BuildHasherDefault<IdentityHasher>>) -> Self {
        match entry {
            hash_map::Entry::Occupied(entry) => Self::Occupied(entry),
            hash_map::Entry::Vacant(entry) => Self::Vacant(entry),
        }
    }
}
