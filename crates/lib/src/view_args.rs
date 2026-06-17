use crate::{Hash, Identity};
use spacetimedb_sats::{bsatn::ToBsatn, AlgebraicValue, ProductValue};

pub const VIEW_ARGS_HASH_DOMAIN: &[u8] = b"spacetimedb::view::args::v1\0";

pub fn hash_view_args(args_bsatn: &[u8]) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(VIEW_ARGS_HASH_DOMAIN);
    hasher.update(args_bsatn);
    Hash::from_byte_array(*hasher.finalize().as_bytes())
}

pub fn hash_empty_view_args() -> Hash {
    let args_bsatn = ProductValue::default()
        .to_bsatn_vec()
        .expect("empty view args should serialize");
    hash_view_args(&args_bsatn)
}

pub fn hash_sender_view_args(sender: Identity) -> Hash {
    let args_bsatn = ProductValue::from_iter([sender.into()])
        .to_bsatn_vec()
        .expect("sender view args should serialize");
    hash_view_args(&args_bsatn)
}

pub fn empty_view_arg_hash_value() -> AlgebraicValue {
    AlgebraicValue::U256(hash_empty_view_args().to_u256().into())
}

pub fn sender_view_arg_hash_value(sender: Identity) -> AlgebraicValue {
    AlgebraicValue::U256(hash_sender_view_args(sender).to_u256().into())
}
