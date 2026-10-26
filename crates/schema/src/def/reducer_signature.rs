use spacetimedb_lib::{bsatn, hash_bytes, Hash};
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, Typespace};
use std::collections::BTreeMap;

const REDUCER_SIGNATURE_HASH_DOMAIN: &[u8] = b"spacetimedb:idc:reducer-signature:v1\0";

/// Computes the IDC signature hash from parameter names, order, types, and both return types.
///
/// The encoding is the versioned domain prefix followed by BSATN
/// `(AlgebraicType::Product(params), ok_return_type, err_return_type, reducer_typespace)`,
/// hashed with Keccak-256. Reducer Typespace are a `Vec<AlgebraicType>` in first-use order,
/// traversing parameters, then the success type, then the error type, kind of reducer local
/// typespace.
pub fn reducer_signature_hash(
    params: &ProductType,
    ok_return_type: &AlgebraicType,
    err_return_type: &AlgebraicType,
    typespace: &Typespace,
) -> Hash {
    fn normalize(
        ty: &mut AlgebraicType,
        typespace: &Typespace,
        refs: &mut BTreeMap<AlgebraicTypeRef, AlgebraicTypeRef>,
        reducer_typespace: &mut Typespace,
    ) {
        match ty {
            AlgebraicType::Ref(original) => {
                // Number reachable types in first-use order so unrelated types and
                // their positions in the module's typespace do not affect the hash.
                let local_idx = if let Some(local_idx) = refs.get(original) {
                    *local_idx
                } else {
                    // Reserve the id before following references so recursive types
                    // can refer back to this definition while it is being copied.
                    let local_idx = reducer_typespace.add(AlgebraicType::unit());
                    refs.insert(*original, local_idx);
                    let mut definition = typespace[*original].clone();
                    normalize(&mut definition, typespace, refs, reducer_typespace);
                    reducer_typespace[local_idx] = definition;
                    local_idx
                };
                *original = local_idx;
            }
            AlgebraicType::Product(product) => {
                for field in &mut product.elements {
                    normalize(&mut field.algebraic_type, typespace, refs, reducer_typespace);
                }
            }
            AlgebraicType::Sum(sum) => {
                for variant in &mut sum.variants {
                    normalize(&mut variant.algebraic_type, typespace, refs, reducer_typespace);
                }
            }
            AlgebraicType::Array(array) => normalize(&mut array.elem_ty, typespace, refs, reducer_typespace),
            _ => {}
        }
    }

    let mut params = AlgebraicType::Product(params.clone());
    let mut ok_return_type = ok_return_type.clone();
    let mut err_return_type = err_return_type.clone();
    let mut refs = BTreeMap::new();
    let mut reducer_typespace = Typespace::default();
    for ty in [&mut params, &mut ok_return_type, &mut err_return_type] {
        normalize(ty, typespace, &mut refs, &mut reducer_typespace);
    }
    let mut bytes = REDUCER_SIGNATURE_HASH_DOMAIN.to_vec();
    bsatn::to_writer(
        &mut bytes,
        &(params, ok_return_type, err_return_type, reducer_typespace.types),
    )
    .expect("validated signature should serialize");
    hash_bytes(&bytes)
}
