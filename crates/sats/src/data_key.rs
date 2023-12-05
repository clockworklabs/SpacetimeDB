use std::fmt::{self, Write};
use std::ops::Deref;

use crate::buffer::{BufReader, BufWriter, DecodeError};
use crate::hash::{hash_bytes, Hash};

#[cfg(any(test, feature = "proptest"))]
use proptest::prelude::*;
#[cfg(any(test, feature = "proptest"))]
use proptest_derive::Arbitrary;

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[cfg_attr(any(test, feature = "proptest"), derive(Arbitrary))]
pub enum DataKey {
    Data(InlineData),
    Hash(Hash),
}

impl DataKey {
    /// The minimum possible value for a DataKey, used for sorting DataKeys
    pub fn min_datakey() -> Self {
        DataKey::Data(InlineData { len: 0, buf: [0; 31] })
    }

    /// The maximum possible value for a DataKey, used for sorting DataKeys
    pub fn max_datakey() -> Self {
        DataKey::Hash(Hash::from_slice(&[255; 32]))
    }
}

const MAX_INLINE: usize = 31;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineData {
    len: u8,
    buf: [u8; MAX_INLINE],
}
impl InlineData {
    #[inline]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let mut buf = [0; MAX_INLINE];
        let sub_buf = buf.get_mut(..b.len())?;
        sub_buf.copy_from_slice(b);
        Some(Self {
            len: b.len() as u8,
            buf,
        })
    }
}
impl Deref for InlineData {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.buf[..self.len as usize]
    }
}
// TODO: figure out why these impls break things
// impl PartialEq for InlineData {
//     fn eq(&self, other: &Self) -> bool {
//         **self == **other
//     }
// }
// impl Eq for InlineData {}
// impl PartialOrd for InlineData {
//     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
//         Some(self.cmp(other))
//     }
// }
// impl Ord for InlineData {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         Ord::cmp(&**self, &**other)
//     }
// }
// impl std::hash::Hash for InlineData {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
//         // should this be hash_bytes(&**self).hash() instead?
//         (**self).hash(state);
//     }
// }
impl fmt::Debug for InlineData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        fmt::Display::fmt(&(**self).escape_ascii(), f)?;
        f.write_char('"')
    }
}

#[cfg(any(test, feature = "proptest"))]
impl Arbitrary for InlineData {
    type Parameters = ();
    type Strategy = prop::strategy::Map<prop::collection::VecStrategy<prop::num::u8::Any>, fn(Vec<u8>) -> InlineData>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop::collection::vec(any::<u8>(), 0..MAX_INLINE).prop_map(|bytes| InlineData::from_bytes(&bytes).unwrap())
    }
}

const IS_HASH_BIT: u8 = 0b1000_0000;

// <flags(1)><value(0-32)>
impl DataKey {
    // Convert a bunch of data to the value that represents it.
    // Throws away the data.
    pub fn from_data(data: impl AsRef<[u8]>) -> Self {
        let data = data.as_ref();
        match InlineData::from_bytes(data) {
            Some(data) => DataKey::Data(data),
            None => DataKey::Hash(hash_bytes(data)),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data_key_summary = Vec::new();
        self.encode(&mut data_key_summary);
        data_key_summary
    }

    pub fn decode<'a>(bytes: &mut impl BufReader<'a>) -> Result<Self, DecodeError> {
        let header = bytes.get_u8()?;

        let is_hash = (header & IS_HASH_BIT) != 0;

        if is_hash {
            // future-proof it, ish
            if header != IS_HASH_BIT {
                return Err(DecodeError::InvalidTag);
            }
            let hash = Hash {
                data: bytes.get_array()?,
            };
            Ok(Self::Hash(hash))
        } else {
            let len = header;
            if len as usize > MAX_INLINE {
                return Err(DecodeError::BufferLength {
                    for_type: "DataKey".into(),
                    expected: MAX_INLINE,
                    given: len as usize,
                });
            }
            let mut buf = [0; MAX_INLINE];
            let data = bytes.get_slice(len as usize)?;
            buf[..len as usize].copy_from_slice(data);
            Ok(Self::Data(InlineData { len, buf }))
        }
    }

    pub fn encoded_len(&self) -> usize {
        // 1 for the header byte
        let mut count = 1;
        count += match self {
            DataKey::Data(data) => data.len(),
            DataKey::Hash(hash) => hash.data.len(),
        };
        count
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        let (header, data) = match self {
            DataKey::Data(data) => (data.len, &**data),
            DataKey::Hash(hash) => (IS_HASH_BIT, &hash.data[..]),
        };
        bytes.put_u8(header);
        bytes.put_slice(data);
    }
}

pub trait ToDataKey {
    fn to_data_key(&self) -> DataKey;
}

impl ToDataKey for crate::AlgebraicValue {
    fn to_data_key(&self) -> DataKey {
        let mut bytes = Vec::new();
        self.encode(&mut bytes);
        DataKey::from_data(&bytes)
    }
}
impl ToDataKey for crate::ProductValue {
    fn to_data_key(&self) -> DataKey {
        let mut bytes = Vec::new();
        self.encode(&mut bytes);
        DataKey::from_data(&bytes)
    }
}
