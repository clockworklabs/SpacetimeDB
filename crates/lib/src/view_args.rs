use crate::{Hash, Identity};
use spacetimedb_sats::{bsatn::ToBsatn, AlgebraicValue, ProductValue};

pub const VIEW_ARGS_HASH_DOMAIN: &[u8] = b"spacetimedb::view::args::v1\0";

/// The hash domain for the arguments of a scoped view's body, i.e. its scope key.
///
/// This is distinct from [`VIEW_ARGS_HASH_DOMAIN`],
/// so that the instance of a scoped view's body for a key
/// never shares an arg hash with the instance of its resolver for a sender,
/// even when the key is itself an `Identity`.
pub const SCOPED_VIEW_ARGS_HASH_DOMAIN: &[u8] = b"spacetimedb::view::scope::v1\0";

/// The hash domain for subscribers of a scoped view whose resolver returned no scope.
pub const UNSCOPED_VIEW_ARGS_HASH_DOMAIN: &[u8] = b"spacetimedb::view::scope::none::v1\0";

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

/// Hash the BSATN-encoded arguments `(key,)` of a scoped view's body.
pub fn hash_scoped_view_args(args_bsatn: &[u8]) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SCOPED_VIEW_ARGS_HASH_DOMAIN);
    hasher.update(args_bsatn);
    Hash::from_byte_array(*hasher.finalize().as_bytes())
}

/// The arg hash selected by a subscriber of a scoped view who is in no scope.
///
/// No materialized rows have this arg hash, so such a subscriber observes an empty view.
pub fn hash_unscoped_view_args() -> Hash {
    Hash::from_byte_array(*blake3::hash(UNSCOPED_VIEW_ARGS_HASH_DOMAIN).as_bytes())
}

pub fn scoped_view_arg_hash_value(args_bsatn: &[u8]) -> AlgebraicValue {
    AlgebraicValue::U256(hash_scoped_view_args(args_bsatn).to_u256().into())
}

pub fn unscoped_view_arg_hash_value() -> AlgebraicValue {
    AlgebraicValue::U256(hash_unscoped_view_args().to_u256().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_hashes_are_domain_separated_from_sender_hashes() {
        let identity = Identity::ONE;
        let args = ProductValue::from_iter([identity.into()]).to_bsatn_vec().unwrap();
        assert_ne!(hash_scoped_view_args(&args), hash_sender_view_args(identity));
        assert_ne!(hash_scoped_view_args(&args), hash_view_args(&args));
        assert_ne!(hash_unscoped_view_args(), hash_empty_view_args());
    }
}
