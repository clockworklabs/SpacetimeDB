//! Defines hash functions for `AlgebraicValue` and friends.

use crate::{
    bsatn::Deserializer,
    buffer::{BufReader, DecodeError},
    de::{Deserialize, Deserializer as _},
    AlgebraicType, AlgebraicValue, ArrayValue, MapType, ProductType, ProductValue, SumType, F32, F64,
};
use core::hash::{Hash, Hasher};

// We only manually implement those hash functions that cannot be `#[derive(Hash)]`ed.
// Those that can be are:
//
// - `sum: SumValue`: The generated impl will first write the `sum.tag: u8`,
//   and then proceed to write the `sum.value`, which delegates to our custom impl below.
//   The tag is hashed so that e.g., `Result<u32, u32>` represented as an AV
//   results in different hashes for `Ok(x)` and `Err(x)`.
//
// - `map: MapValue`: Uses the hash function for `BTreeMap<AV, AV>`,
//   which is length prefixed and then writes each `(key, value)` sequentially.
//   Eventually, this delegates to our custom impl below.
//
// - `str: Box<str>`: Uses the standard hash function for `str`.
//
// - Primitive types: Trivially what we want.

/// The hash function for an [`AlgebraicValue`] only hashes its domain types
/// and avoids length prefixing for product values.
/// This avoids the hashing `Discriminant<AlgebraicValue>`
/// which is OK as a table column will only ever have the same type (and so the same discriminant).
impl Hash for AlgebraicValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            AlgebraicValue::Sum(x) => x.hash(state),
            AlgebraicValue::Product(x) => x.hash(state),
            AlgebraicValue::Array(x) => x.hash(state),
            AlgebraicValue::Map(x) => x.hash(state),
            AlgebraicValue::Bool(x) => x.hash(state),
            AlgebraicValue::I8(x) => x.hash(state),
            AlgebraicValue::U8(x) => x.hash(state),
            AlgebraicValue::I16(x) => x.hash(state),
            AlgebraicValue::U16(x) => x.hash(state),
            AlgebraicValue::I32(x) => x.hash(state),
            AlgebraicValue::U32(x) => x.hash(state),
            AlgebraicValue::I64(x) => x.hash(state),
            AlgebraicValue::U64(x) => x.hash(state),
            AlgebraicValue::I128(x) => x.hash(state),
            AlgebraicValue::U128(x) => x.hash(state),
            AlgebraicValue::F32(x) => x.hash(state),
            AlgebraicValue::F64(x) => x.hash(state),
            AlgebraicValue::String(s) => s.hash(state),
        }
    }
}

/// The hash function for `ProductValue` does *not* length prefix.
impl Hash for ProductValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for field in self.elements.iter() {
            field.hash(state);
        }
    }
}

/// The hash function for an [`ArrayValue`] only hashes its domain types.
/// This avoids the hashing `Discriminant<ArrayValue>`
/// which is OK as a table column will only ever have the same type (and so the same discriminant).
/// The hash function will however length-prefix as the value is of variable length.
impl Hash for ArrayValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ArrayValue::Sum(es) => es.hash(state),
            ArrayValue::Product(es) => es.hash(state),
            ArrayValue::Bool(es) => es.hash(state),
            ArrayValue::I8(es) => es.hash(state),
            ArrayValue::U8(es) => es.hash(state),
            ArrayValue::I16(es) => es.hash(state),
            ArrayValue::U16(es) => es.hash(state),
            ArrayValue::I32(es) => es.hash(state),
            ArrayValue::U32(es) => es.hash(state),
            ArrayValue::I64(es) => es.hash(state),
            ArrayValue::U64(es) => es.hash(state),
            ArrayValue::I128(es) => es.hash(state),
            ArrayValue::U128(es) => es.hash(state),
            ArrayValue::F32(es) => es.hash(state),
            ArrayValue::F64(es) => es.hash(state),
            ArrayValue::String(es) => es.hash(state),
            ArrayValue::Array(es) => es.hash(state),
            ArrayValue::Map(es) => es.hash(state),
        }
    }
}

type HR = Result<(), DecodeError>;

pub fn hash_bsatn<'a>(state: &mut impl Hasher, ty: &AlgebraicType, de: Deserializer<'_, impl BufReader<'a>>) -> HR {
    match ty {
        AlgebraicType::Ref(_) => unreachable!("hash_bsatn does not have a typespace"),
        AlgebraicType::Sum(ty) => hash_bsatn_sum(state, ty, de),
        AlgebraicType::Product(ty) => hash_bsatn_prod(state, ty, de),
        AlgebraicType::Array(ty) => hash_bsatn_array(state, &ty.elem_ty, de),
        AlgebraicType::Map(ty) => hash_bsatn_map(state, ty, de),
        AlgebraicType::Bool => hash_bsatn_de::<bool>(state, de),
        AlgebraicType::I8 => hash_bsatn_de::<i8>(state, de),
        AlgebraicType::U8 => hash_bsatn_de::<u8>(state, de),
        AlgebraicType::I16 => hash_bsatn_de::<i16>(state, de),
        AlgebraicType::U16 => hash_bsatn_de::<u16>(state, de),
        AlgebraicType::I32 => hash_bsatn_de::<i32>(state, de),
        AlgebraicType::U32 => hash_bsatn_de::<u32>(state, de),
        AlgebraicType::I64 => hash_bsatn_de::<i64>(state, de),
        AlgebraicType::U64 => hash_bsatn_de::<u64>(state, de),
        AlgebraicType::I128 => hash_bsatn_de::<i128>(state, de),
        AlgebraicType::U128 => hash_bsatn_de::<u128>(state, de),
        AlgebraicType::F32 => hash_bsatn_de::<F32>(state, de),
        AlgebraicType::F64 => hash_bsatn_de::<F64>(state, de),
        AlgebraicType::String => hash_bsatn_de::<&str>(state, de),
    }
}

/// Hashes the tag and payload of the BSATN-encoded sum value.
fn hash_bsatn_sum<'a>(state: &mut impl Hasher, ty: &SumType, mut de: Deserializer<'_, impl BufReader<'a>>) -> HR {
    // Read + hash the tag.
    let tag = de.reborrow().deserialize_u8()?;
    tag.hash(state);

    // Hash the payload.
    let data_ty = &ty.variants[tag as usize].algebraic_type;
    hash_bsatn(state, data_ty, de)
}

/// Hashes every field in the BSATN-encoded product value.
fn hash_bsatn_prod<'a>(state: &mut impl Hasher, ty: &ProductType, mut de: Deserializer<'_, impl BufReader<'a>>) -> HR {
    ty.elements
        .iter()
        .try_for_each(|f| hash_bsatn(state, &f.algebraic_type, de.reborrow()))
}

/// Hashes every elem in the BSATN-encoded array value.
fn hash_bsatn_array<'a>(state: &mut impl Hasher, ty: &AlgebraicType, de: Deserializer<'_, impl BufReader<'a>>) -> HR {
    // The BSATN is length-prefixed.
    // `Hash for &[T]` also does length-prefixing.
    match ty {
        AlgebraicType::Ref(_) => unreachable!("hash_bsatn does not have a typespace"),
        AlgebraicType::Sum(ty) => hash_bsatn_seq(state, de, |s, d| hash_bsatn_sum(s, ty, d)),
        AlgebraicType::Product(ty) => hash_bsatn_seq(state, de, |s, d| hash_bsatn_prod(s, ty, d)),
        AlgebraicType::Array(ty) => hash_bsatn_seq(state, de, |s, d| hash_bsatn_array(s, &ty.elem_ty, d)),
        AlgebraicType::Map(ty) => hash_bsatn_seq(state, de, |s, d| hash_bsatn_map(s, ty, d)),
        AlgebraicType::Bool => hash_bsatn_seq(state, de, hash_bsatn_de::<bool>),
        AlgebraicType::I8 | AlgebraicType::U8 => hash_bsatn_int_seq(state, de, 1),
        AlgebraicType::I16 | AlgebraicType::U16 => hash_bsatn_int_seq(state, de, 2),
        AlgebraicType::I32 | AlgebraicType::U32 => hash_bsatn_int_seq(state, de, 4),
        AlgebraicType::I64 | AlgebraicType::U64 => hash_bsatn_int_seq(state, de, 8),
        AlgebraicType::I128 | AlgebraicType::U128 => hash_bsatn_int_seq(state, de, 16),
        AlgebraicType::F32 => hash_bsatn_seq(state, de, hash_bsatn_de::<F32>),
        AlgebraicType::F64 => hash_bsatn_seq(state, de, hash_bsatn_de::<F64>),
        AlgebraicType::String => hash_bsatn_seq(state, de, hash_bsatn_de::<&str>),
    }
}

/// Hashes every (key, value) in the BSATN-encoded map value.
fn hash_bsatn_map<'a>(state: &mut impl Hasher, ty: &MapType, de: Deserializer<'_, impl BufReader<'a>>) -> HR {
    // Hash each (key, value) pair but first length-prefix.
    // This is OK as BSATN serializes the map in order
    // and `BTreeMap` will hash the elements in order,
    // so everything stays consistent.
    hash_bsatn_seq(state, de, |state, mut de| {
        hash_bsatn(state, &ty.key_ty, de.reborrow())?;
        hash_bsatn(state, &ty.ty, de)?;
        Ok(())
    })
}

/// Hashes elements in the BSATN-encoded element sequence.
/// The sequence is prefixed with its length and the hash will as well.
fn hash_bsatn_seq<'a, H: Hasher, R: BufReader<'a>>(
    state: &mut H,
    mut de: Deserializer<'_, R>,
    mut elem_hash: impl FnMut(&mut H, Deserializer<'_, R>) -> Result<(), DecodeError>,
) -> HR {
    // The BSATN is length-prefixed.
    // The Hash also needs to be length-prefixed.
    let len = de.reborrow().deserialize_len()?;
    state.write_usize(len);

    // Hash each element.
    (0..len).try_for_each(|_| elem_hash(state, de.reborrow()))
}

/// Hashes the BSATN-encoded integer sequence where each integer is `width` bytes wide.
/// The sequence is prefixed with its length and the hash will as well.
fn hash_bsatn_int_seq<'a, H: Hasher, R: BufReader<'a>>(state: &mut H, mut de: Deserializer<'_, R>, width: usize) -> HR {
    // The BSATN is length-prefixed.
    // The Hash also needs to be length-prefixed.
    let len = de.reborrow().deserialize_len()?;
    state.write_usize(len);

    // Extract and hash the bytes.
    //´This is consistent with what `<$int_primitive>::hash_slice` will do.
    let bytes = de.get_slice(len * width)?;
    state.write(bytes);
    Ok(())
}

/// Deserializes from `de` an `x: T` and then proceeds to hash `x`.
fn hash_bsatn_de<'a, T: Hash + Deserialize<'a>>(
    state: &mut impl Hasher,
    de: Deserializer<'_, impl BufReader<'a>>,
) -> HR {
    T::deserialize(de).map(|x| x.hash(state))
}

#[cfg(test)]
mod tests {
    use crate::{
        bsatn::{to_vec, Deserializer},
        hash_bsatn,
        proptest::generate_typed_value,
        AlgebraicType, AlgebraicValue,
    };
    use proptest::prelude::*;
    use std::hash::{BuildHasher, Hasher as _};

    fn hash_one_bsatn_av(bh: &impl BuildHasher, ty: &AlgebraicType, val: &AlgebraicValue) -> u64 {
        let mut bsatn = &*to_vec(&val).unwrap();
        let de = Deserializer::new(&mut bsatn);
        let mut hasher = bh.build_hasher();
        hash_bsatn(&mut hasher, ty, de).unwrap();
        hasher.finish()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2048))]
        #[test]
        fn av_bsatn_hash_same_std_random_state((ty, val) in generate_typed_value()) {
            let rs = std::hash::RandomState::new();
            let hash_av = rs.hash_one(&val);
            let hash_av_bsatn = hash_one_bsatn_av(&rs, &ty, &val);
            prop_assert_eq!(hash_av, hash_av_bsatn);
        }

        #[test]
        fn av_bsatn_hash_same_ahash((ty, val) in generate_typed_value()) {
            let rs = ahash::RandomState::new();
            let hash_av = rs.hash_one(&val);
            let hash_av_bsatn = hash_one_bsatn_av(&rs, &ty, &val);
            prop_assert_eq!(hash_av, hash_av_bsatn);
        }
    }
}
