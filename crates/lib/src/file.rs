//! Types shared between the host and modules for File Storage.
//!
//! A file is an immutable sequence of bytes, stored outside the database in an object store
//! under the BLAKE3 hash of its contents. The database records each file as a row in `st_file`,
//! which is the only strong reference to the file's bytes.
//!
//! Modules and clients refer to files with two weak values:
//!
//! - A [`FileId`] names a file but does not grant access to its contents.
//! - A [`FileRef`] is a [`FileId`] plus the file's read secret.
//!   Holding a `FileRef` is what lets a client download the file.

use crate::{Hash, Timestamp};
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, u256, AlgebraicType, SpacetimeType};
use std::{fmt, str::FromStr};

/// A unique random identifier for a file.
///
/// A `FileId` does not grant access to the file's contents.
/// Store a `FileId` instead of a [`FileRef`] in rows that are visible to more people than the file should be.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FileId(pub u128);

impl_st!([] FileId, AlgebraicType::U128);
impl_serialize!([] FileId, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] FileId, de => <u128>::deserialize(de).map(FileId));

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({:032x})", self.0)
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// A read capability for a file: its [`FileId`] plus its read secret.
///
/// Anyone holding a `FileRef` can download the file's contents,
/// so treat a `FileRef` the way you would treat the bytes themselves.
///
/// Represented in SATS as a `U256`, with the `FileId` in the low 128 bits
/// and the read secret in the high 128 bits.
/// Its text form, used in URLs, is the `FileId` in hex followed by the read secret in hex.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileRef {
    id: FileId,
    read_secret: u128,
}

impl_st!([] FileRef, AlgebraicType::U256);
impl_serialize!([] FileRef, (self, ser) => self.to_u256().serialize(ser));
impl_deserialize!([] FileRef, de => <u256>::deserialize(de).map(FileRef::from_u256));

impl FileRef {
    pub const fn new(id: FileId, read_secret: u128) -> Self {
        Self { id, read_secret }
    }

    /// The file this reference names.
    pub const fn id(&self) -> FileId {
        self.id
    }

    /// The secret that grants read access.
    pub const fn read_secret(&self) -> u128 {
        self.read_secret
    }

    pub fn to_u256(&self) -> u256 {
        u256::from_words(self.read_secret, self.id.0)
    }

    pub fn from_u256(value: u256) -> Self {
        let (read_secret, id) = value.into_words();
        Self::new(FileId(id), read_secret)
    }

    /// The path segment that names this file in the File Storage HTTP API.
    pub fn to_url_segment(&self) -> String {
        self.to_string()
    }
}

impl fmt::Debug for FileRef {
    // Never print the secret in debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileRef").field("id", &self.id).finish_non_exhaustive()
    }
}

impl fmt::Display for FileRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}{:032x}", self.id.0, self.read_secret)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseFileRefError;

impl fmt::Display for ParseFileRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a file reference is 64 hexadecimal digits")
    }
}

impl std::error::Error for ParseFileRefError {}

impl FromStr for FileRef {
    type Err = ParseFileRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseFileRefError);
        }
        let id = u128::from_str_radix(&s[..32], 16).map_err(|_| ParseFileRefError)?;
        let read_secret = u128::from_str_radix(&s[32..], 16).map_err(|_| ParseFileRefError)?;
        Ok(Self::new(FileId(id), read_secret))
    }
}

/// The BLAKE3 hash of a file's contents, which is also the file's key in the object store.
pub fn content_hash(bytes: &[u8]) -> Hash {
    Hash::from_byte_array(*blake3::hash(bytes).as_bytes())
}

/// What the host knows about a file, as returned to modules and clients.
///
/// This mirrors a row of `st_file`, without making `st_file` itself part of the module API.
#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
pub struct FileMetadata {
    pub file_ref: FileRef,
    /// The BLAKE3 hash of the file's contents.
    pub content_hash: Hash,
    pub size: u64,
    pub created_at: Timestamp,
    /// The content type the uploader or module declared. Nothing verifies it.
    pub content_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::bsatn;

    #[test]
    fn file_ref_round_trips_through_text_and_bsatn() {
        let file_ref = FileRef::new(FileId(0x0123_4567_89ab_cdef_0011_2233_4455_6677), u128::MAX - 7);
        let text = file_ref.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(text.parse::<FileRef>(), Ok(file_ref));

        let bytes = bsatn::to_vec(&file_ref).unwrap();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bsatn::from_slice::<FileRef>(&bytes).unwrap(), file_ref);
        assert_eq!(FileRef::from_u256(file_ref.to_u256()), file_ref);
    }

    #[test]
    fn file_ref_debug_hides_the_secret() {
        let file_ref = FileRef::new(FileId(1), 0xdead_beef);
        assert!(!format!("{file_ref:?}").contains("deadbeef"));
    }

    #[test]
    fn file_ref_rejects_malformed_text() {
        assert_eq!("".parse::<FileRef>(), Err(ParseFileRefError));
        assert_eq!("zz".repeat(32).parse::<FileRef>(), Err(ParseFileRefError));
        assert_eq!("0".repeat(63).parse::<FileRef>(), Err(ParseFileRefError));
    }
}
