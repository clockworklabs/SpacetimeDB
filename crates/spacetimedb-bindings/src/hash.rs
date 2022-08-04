use sha3::{Digest, Keccak256};

#[derive(Eq, PartialEq, Clone, Copy, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub struct Hash {
    pub data: [u8; 32],
}

impl Hash {
    pub fn from_arr(arr: &[u8; 32]) -> Self {
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
    let mut hasher = Keccak256::new();
    hasher.update(bytes);
    let data: [u8; 32] = hasher.finalize().try_into().unwrap();
    Hash { data }
}
