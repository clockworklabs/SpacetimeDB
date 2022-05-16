use sha3::{
    digest::{generic_array::typenum::U32, generic_array::GenericArray},
    Digest, Keccak256,
};
pub type Hash = GenericArray<u8, U32>;

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    let mut hasher = Keccak256::new();
    hasher.update(bytes);
    hasher.finalize()
}
