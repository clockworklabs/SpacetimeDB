use crate::{Hash, Identity};
use spacetimedb_sats::{bsatn, bsatn::ToBsatn, AlgebraicValue, ProductValue};

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

/// Hashes the tuple `[sender?, args...]` for a view instance.
///
/// BSATN encodes a product as the concatenation of its elements,
/// so writing `sender` then `args` produces the BSATN of that combined tuple
/// without building it, and this reduces to [`hash_empty_view_args`] /
/// [`hash_sender_view_args`] when `args` is empty.
pub fn hash_view_instance_args(sender: Option<Identity>, args: &ProductValue) -> Hash {
    let mut args_bsatn = Vec::new();
    if let Some(sender) = sender {
        bsatn::to_writer(&mut args_bsatn, &AlgebraicValue::from(sender)).expect("sender view args should serialize");
    }
    bsatn::to_writer(&mut args_bsatn, args).expect("view instance args should serialize");
    hash_view_args(&args_bsatn)
}

pub fn view_instance_arg_hash_value(sender: Option<Identity>, args: &ProductValue) -> AlgebraicValue {
    AlgebraicValue::U256(hash_view_instance_args(sender, args).to_u256().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::product;

    #[test]
    fn empty_args_match_legacy_hashes() {
        assert_eq!(
            hash_view_instance_args(None, &ProductValue::default()),
            hash_empty_view_args()
        );
        for sender in [Identity::ZERO, Identity::ONE] {
            assert_eq!(
                hash_view_instance_args(Some(sender), &ProductValue::default()),
                hash_sender_view_args(sender)
            );
        }
    }

    #[test]
    fn streamed_hash_matches_combined_tuple() {
        let args = product![42u32, "hello"];

        let combined = ProductValue::from_iter(args.elements.iter().cloned());
        assert_eq!(
            hash_view_instance_args(None, &args),
            hash_view_args(&combined.to_bsatn_vec().unwrap())
        );

        let sender = Identity::ONE;
        let combined = ProductValue::from_iter(std::iter::once(sender.into()).chain(args.elements.iter().cloned()));
        assert_eq!(
            hash_view_instance_args(Some(sender), &args),
            hash_view_args(&combined.to_bsatn_vec().unwrap())
        );
    }

    #[test]
    fn distinct_args_hash_differently() {
        assert_ne!(
            hash_view_instance_args(None, &product![1u32]),
            hash_view_instance_args(None, &product![2u32])
        );
        assert_ne!(
            hash_view_instance_args(Some(Identity::ONE), &product![1u32]),
            hash_view_instance_args(Some(Identity::ONE), &product![2u32])
        );
        assert_eq!(
            hash_view_instance_args(Some(Identity::ONE), &product![1u32]),
            hash_view_instance_args(Some(Identity::ONE), &product![1u32])
        );
    }
}
