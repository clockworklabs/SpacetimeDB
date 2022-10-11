use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

pub const HASH_SIZE: usize = 32;

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize)]
pub struct Hash {
    pub data: [u8; HASH_SIZE],
}

impl Hash {
    pub fn from_arr(arr: &[u8; HASH_SIZE]) -> Self {
        Self { data: arr.clone() }
    }

    pub fn from_slice(slice: &[u8]) -> Self {
        Self {
            data: slice.try_into().unwrap(),
        }
    }
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    let data: [u8; HASH_SIZE] = Keccak256::digest(bytes).into();
    Hash { data }
}
