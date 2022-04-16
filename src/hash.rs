use sha3::{Digest, Sha3_256, digest::{generic_array::GenericArray, generic_array::typenum::U32}};
pub type Hash = GenericArray<u8, U32>;

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    let mut hasher = Sha3_256::new();
    hasher.update(bytes);
    hasher.finalize()
}