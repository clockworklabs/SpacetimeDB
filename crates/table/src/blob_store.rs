//! Defines the content address used for large var-len objects.

use blake3::hash;
use core::fmt;
use spacetimedb_lib::{de::Deserialize, ser::Serialize};
use spacetimedb_memory_usage::MemoryUsage;
use std::fmt::Write;

/// The content address of a blob-stored object.
#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash, Debug, Serialize, Deserialize)]
pub struct BlobHash {
    /// The hash of the blob-stored object.
    ///
    /// Uses BLAKE3 which fits in 32 bytes.
    pub data: [u8; BlobHash::SIZE],
}

impl MemoryUsage for BlobHash {}

impl BlobHash {
    pub const SIZE: usize = 32;

    pub fn hash_from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: hash(bytes).into(),
        }
    }
}

impl TryFrom<&[u8]> for BlobHash {
    type Error = ();

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self {
            data: data.try_into().map_err(drop)?,
        })
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const ALPHABET: &[u8; 16] = b"0123456789abcdef";
        for &b in &self.data {
            f.write_char(ALPHABET[(b >> 4) as usize] as char)?;
            f.write_char(ALPHABET[(b & 0xf) as usize] as char)?;
        }
        Ok(())
    }
}
