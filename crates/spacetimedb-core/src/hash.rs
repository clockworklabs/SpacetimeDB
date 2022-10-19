use crate::error::DBError;
use sha3::{Digest, Keccak256};

#[derive(Eq, PartialEq, Clone, Copy, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub struct Hash {
    pub data: [u8; 32],
}

impl Hash {
    const ABBREVIATION_LEN: usize = 16;

    pub fn from_arr(arr: &[u8; 32]) -> Self {
        Self { data: arr.clone() }
    }

    pub fn from_hex(hex: &str) -> Result<Self, DBError> {
        let data = hex::decode(hex)?;
        let data: [u8; 32] = data.try_into().map_err(|e: Vec<_>| DBError::DecodeHexHash(e.len()))?;
        Ok(Self { data })
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.data)
    }

    pub fn to_abbreviated_hex(&self) -> String {
        self.to_hex()[0..Hash::ABBREVIATION_LEN].to_owned()
    }

    pub fn from_slice(slice: impl AsRef<[u8]>) -> Self {
        let slice = slice.as_ref();
        Self {
            data: slice.try_into().unwrap(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data.as_slice()
    }
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    let mut hasher = Keccak256::new();
    hasher.update(bytes);
    let data: [u8; 32] = hasher.finalize().try_into().unwrap();
    Hash { data }
}

pub trait ToHexString {
    fn to_hex_string(&self) -> String;
}

impl ToHexString for Hash {
    fn to_hex_string(&self) -> String {
        self.to_hex()
    }
}
