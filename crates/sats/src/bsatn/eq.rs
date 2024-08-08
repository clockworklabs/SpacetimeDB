//! Defines the function [`eq_bsatn`] which equates `lhs: &AlgebraicValue` to `rhs` defined in BSATN.
//!
//! The lifetime `'r` in `eq_bsatn` is the lifetime of `rhs`'s backing data, i.e., the BSATN itself.

use super::Deserializer;
use crate::{buffer::BufReader, de::Deserialize, AlgebraicValue, ArrayValue, MapValue, ProductValue, SumValue};
use core::{mem, slice};

/// Equates `lhs` to a BSATN-encoded `AlgebraicValue` of the same type.
pub fn eq_bsatn<'r>(lhs: &AlgebraicValue, rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    match lhs {
        AlgebraicValue::Sum(lhs) => eq_bsatn_sum(lhs, rhs),
        AlgebraicValue::Product(lhs) => eq_bsatn_prod(lhs, rhs),
        AlgebraicValue::Array(lhs) => eq_bsatn_array(lhs, rhs),
        AlgebraicValue::Map(lhs) => eq_bsatn_map(lhs, rhs),
        AlgebraicValue::Bool(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::I8(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::U8(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::I16(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::U16(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::I32(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::U32(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::I64(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::U64(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::I128(lhs) => eq_bsatn_de(&{ lhs.0 }, rhs),
        AlgebraicValue::U128(lhs) => eq_bsatn_de(&{ lhs.0 }, rhs),
        AlgebraicValue::I256(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::U256(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::F32(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::F64(lhs) => eq_bsatn_de(lhs, rhs),
        AlgebraicValue::String(lhs) => eq_bsatn_str(lhs, rhs),
    }
}

/// Equates the tag and payload to that of the BSATN-encoded sum value.
fn eq_bsatn_sum<'r>(lhs: &SumValue, mut rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    eq_bsatn_de(&lhs.tag, rhs.reborrow()) && eq_bsatn(&lhs.value, rhs)
}

/// Equates every field `lhs` to those in the BSATN-encoded product value.
fn eq_bsatn_prod<'r>(lhs: &ProductValue, mut rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    lhs.elements.iter().all(|f| eq_bsatn(f, rhs.reborrow()))
}

/// Equates `lhs` to the `(key, value)`s in the BSATN-encoded map value.
fn eq_bsatn_map<'r>(lhs: &MapValue, rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    eq_bsatn_seq(lhs, rhs, |(key, value), mut rhs| {
        eq_bsatn(key, rhs.reborrow()) && eq_bsatn(value, rhs)
    })
}

/// Equates every elem in `lhs` to those in the BSATN-encoded array value.
fn eq_bsatn_array<'r>(lhs: &ArrayValue, rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    match lhs {
        ArrayValue::Sum(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_sum),
        ArrayValue::Product(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_prod),
        ArrayValue::Bool(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_de),
        ArrayValue::F32(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_de),
        ArrayValue::F64(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_de),
        ArrayValue::String(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_str),
        ArrayValue::Array(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_array),
        ArrayValue::Map(lhs) => eq_bsatn_seq(&**lhs, rhs, eq_bsatn_map),
        // SAFETY: For all of the below, the element types are integer types, as required.
        ArrayValue::I8(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::U8(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::I16(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::U16(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::I32(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::U32(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::I64(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::U64(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::I128(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::U128(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::I256(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
        ArrayValue::U256(lhs) => unsafe { eq_bsatn_int_seq(lhs, rhs) },
    }
}

/// Equates the integer slice `lhs` to the BSATN-encoded one in `rhs`.
///
/// SAFETY: `T` must be an integer type.
unsafe fn eq_bsatn_int_seq<'r, T>(lhs: &[T], mut rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    // The BSATN is length-prefixed.
    let Ok(len) = rhs.reborrow().deserialize_len() else {
        return false;
    };

    // Extract the rhs bytes.
    let Ok(rhs_bytes) = rhs.get_slice(len * mem::size_of::<T>()) else {
        return false;
    };

    // Convert `lhs` to `&[u8]`.
    let ptr = lhs.as_ptr().cast::<u8>();
    // SAFETY: Caller promised that `T` is an integer type.
    // Thus it has no safety requirements and no padding,
    // so it is legal to convert `&[IntType] -> &[u8]`.
    let lhs_bytes = unsafe { slice::from_raw_parts(ptr, mem::size_of_val(lhs)) };

    lhs_bytes == rhs_bytes
}

/// Equates the string `lhs` to the BSATN-encoded one in `rhs`.
#[allow(clippy::borrowed_box)]
fn eq_bsatn_str<'r>(lhs: &Box<str>, rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    <&str>::deserialize(rhs).map(|rhs| &**lhs == rhs).unwrap_or(false)
}

/// Equates elements in `lhs` to the BSATN-encoded element sequence in `rhs`.
/// The sequence is prefixed with its length.
fn eq_bsatn_seq<'r, T, I: ExactSizeIterator<Item = T>, R: BufReader<'r>>(
    lhs: impl IntoIterator<IntoIter = I>,
    mut rhs: Deserializer<'_, R>,
    elem_eq: impl Fn(T, Deserializer<'_, R>) -> bool,
) -> bool {
    let mut lhs = lhs.into_iter();
    // The BSATN is length-prefixed.
    // Compare against length first.
    match rhs.reborrow().deserialize_len() {
        Ok(len) if lhs.len() == len => lhs.all(|e| elem_eq(e, rhs.reborrow())),
        _ => false,
    }
}

/// Deserializes from `de` an `rhs: T` and then proceeds to `lhs == rhs`.
fn eq_bsatn_de<'r, T: Eq + Deserialize<'r>>(lhs: &T, rhs: Deserializer<'_, impl BufReader<'r>>) -> bool {
    T::deserialize(rhs).map(|rhs| lhs == &rhs).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::eq_bsatn;
    use crate::{
        bsatn::{to_vec, Deserializer},
        proptest::generate_typed_value,
    };
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2048))]
        #[test]
        fn encoded_val_eq_to_self((_, val) in generate_typed_value()) {
            let mut bsatn = &*to_vec(&val).unwrap();
            let de = Deserializer::new(&mut bsatn);
            prop_assert!(eq_bsatn(&val, de));
        }
    }
}
