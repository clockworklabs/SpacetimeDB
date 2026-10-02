use core::mem::MaybeUninit;

use crate::buffer::{BufReader, BufWriter, CountWriter};
use crate::de::{BasicSmallVecVisitor, Deserialize, DeserializeSeed, Deserializer as _};
use crate::ser::Serialize;
use crate::{ProductValue, Typespace, WithTypespace};
use bytes::BytesMut;
use ser::BsatnError;
use smallvec::SmallVec;

pub mod de;
pub mod eq;
pub mod ser;

pub use de::Deserializer;
pub use ser::Serializer;

pub use crate::buffer::DecodeError;
pub use ser::BsatnError as EncodeError;

/// Serialize `value` into the buffered writer `w` in the BSATN format.
#[inline]
pub fn to_writer<W: BufWriter, T: Serialize + ?Sized>(w: &mut W, value: &T) -> Result<(), EncodeError> {
    value.serialize_into_bsatn(Serializer::new(w))
}

/// Serialize `value` into a `Vec<u8>` in the BSATN format.
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let mut v = Vec::new();
    to_writer(&mut v, value)?;
    Ok(v)
}

/// Computes the size of `val` when BSATN encoding without actually encoding.
pub fn to_len<T: Serialize + ?Sized>(value: &T) -> Result<usize, EncodeError> {
    let mut writer = CountWriter::default();
    to_writer(&mut writer, value)?;
    Ok(writer.finish())
}

/// Deserialize a `T` from the BSATN format in the buffered `reader`.
pub fn from_reader<'de, T: Deserialize<'de>>(reader: &mut impl BufReader<'de>) -> Result<T, DecodeError> {
    T::deserialize(Deserializer::new(reader))
}

/// Deserialize a `T` from the BSATN format in `bytes`.
pub fn from_slice<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, DecodeError> {
    from_reader(&mut &*bytes)
}

/// Decode `bytes` to the value type of `ty: S`.
pub fn decode<'a, 'de, S: ?Sized>(
    ty: &'a S,
    bytes: &mut impl BufReader<'de>,
) -> Result<<WithTypespace<'a, S> as DeserializeSeed<'de>>::Output, DecodeError>
where
    WithTypespace<'a, S>: DeserializeSeed<'de>,
{
    crate::WithTypespace::empty(ty).deserialize(Deserializer::new(bytes))
}

macro_rules! codec_funcs {
    ($ty:ty) => {
        impl $ty {
            pub fn decode<'a>(bytes: &mut impl BufReader<'a>) -> Result<Self, DecodeError> {
                from_reader(bytes)
            }

            pub fn encode(&self, bytes: &mut impl BufWriter) {
                to_writer(bytes, self).unwrap()
            }
        }
    };
    (val: $ty:ty) => {
        impl $ty {
            /// Decode a value from `bytes` typed at `ty`.
            pub fn decode<'a>(
                ty: &<Self as crate::Value>::Type,
                bytes: &mut impl BufReader<'a>,
            ) -> Result<Self, DecodeError> {
                decode(ty, bytes)
            }

            /// Decode a vector of values from `bytes` with each value typed at `ty`.
            pub fn decode_smallvec<'a>(
                ty: &<Self as crate::Value>::Type,
                bytes: &mut impl BufReader<'a>,
            ) -> Result<SmallVec<[Self; 1]>, DecodeError> {
                Deserializer::new(bytes).deserialize_array_seed(
                    BasicSmallVecVisitor,
                    crate::WithTypespace::new(&Typespace::new(Vec::new()), ty),
                )
            }

            pub fn encode(&self, bytes: &mut impl BufWriter) {
                to_writer(bytes, self).unwrap()
            }
        }
    };
}

codec_funcs!(crate::AlgebraicType);
codec_funcs!(crate::ProductType);
codec_funcs!(crate::SumType);
codec_funcs!(crate::ProductTypeElement);
codec_funcs!(crate::SumTypeVariant);

codec_funcs!(val: crate::AlgebraicValue);
codec_funcs!(val: crate::ProductValue);
codec_funcs!(val: crate::SumValue);

/// Provides a view over a buffer that can reserve an additional `len` bytes
/// and then provide those as an uninitialized buffer to write into.
pub trait BufReservedFill {
    /// Reserves space for `len` in `self` and then runs `fill` to fill it,
    /// adding `len` to the total length of `self`.
    ///
    /// # Safety
    ///
    /// `fill` must initialize every byte in the slice.
    unsafe fn reserve_and_fill(&mut self, len: usize, fill: impl FnOnce(&mut [MaybeUninit<u8>]));
}

impl BufReservedFill for Vec<u8> {
    unsafe fn reserve_and_fill(&mut self, len: usize, fill: impl FnOnce(&mut [MaybeUninit<u8>])) {
        // Get an uninitialized slice within `self` of `len` bytes.
        let start = self.len();
        self.reserve(len);
        let sink = &mut self.spare_capacity_mut()[..len];

        // Run the filling logic.
        fill(sink);

        // SAFETY: Caller promised that `sink` was fully initialized,
        // which entails that we initialized `start .. start + len`,
        // so now we have initialized up to `start + len`.
        unsafe { self.set_len(start + len) }
    }
}

impl BufReservedFill for BytesMut {
    unsafe fn reserve_and_fill(&mut self, len: usize, fill: impl FnOnce(&mut [MaybeUninit<u8>])) {
        // Get an uninitialized slice within `self` of `len` bytes.
        let start = self.len();
        self.reserve(len);
        let sink = &mut self.spare_capacity_mut()[..len];

        // Run the filling logic.
        fill(sink);

        // SAFETY: Caller promised that `sink` was fully initialized,
        // which entails that we initialized `start .. start + len`,
        // so now we have initialized up to `start + len`.
        unsafe { self.set_len(start + len) }
    }
}

/// Types that can be encoded to BSATN.
///
/// Implementations of this trait may be more efficient than directly calling [`to_vec`].
/// In particular, for `spacetimedb_table::table::RowRef`, this method will use a `StaticLayout` if one is available,
/// avoiding expensive runtime type dispatch.
pub trait ToBsatn {
    /// BSATN-encode the row referred to by `self` into a freshly-allocated `Vec<u8>`.
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError>;

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf`, similar to [`Vec::extend`].
    fn to_bsatn_extend(&self, buf: &mut (impl BufWriter + BufReservedFill)) -> Result<(), BsatnError>;

    /// Returns the static size of the type of this object.
    ///
    /// When this returns `Some(_)` there is also a `StaticLayout`.
    fn static_bsatn_size(&self) -> Option<u16>;
}

impl<T: ToBsatn> ToBsatn for &T {
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        T::to_bsatn_vec(*self)
    }
    fn to_bsatn_extend(&self, buf: &mut (impl BufWriter + BufReservedFill)) -> Result<(), BsatnError> {
        T::to_bsatn_extend(*self, buf)
    }
    fn static_bsatn_size(&self) -> Option<u16> {
        T::static_bsatn_size(*self)
    }
}

impl ToBsatn for ProductValue {
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        to_vec(self)
    }

    fn to_bsatn_extend(&self, buf: &mut (impl BufWriter + BufReservedFill)) -> Result<(), BsatnError> {
        to_writer(buf, self)
    }

    fn static_bsatn_size(&self) -> Option<u16> {
        None
    }
}

mod private_is_primitive_type {
    pub trait Sealed {}
}
/// A primitive type.
/// This is purely intended for use in `crates/bindings-macro`.
///
/// # Safety
///
/// Implementing this guarantees that the type has no padding, recursively.
#[doc(hidden)]
pub unsafe trait IsPrimitiveType: private_is_primitive_type::Sealed {}
macro_rules! is_primitive_type {
    ($($prim:ty),*) => {
        $(
            impl private_is_primitive_type::Sealed for $prim {}
            // SAFETY:  the type is primitive and has no padding.
            unsafe impl IsPrimitiveType for $prim {}
        )*
    };
}
is_primitive_type!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64);

/// Enforces that a type is a primitive.
/// This is purely intended for use in `crates/bindings-macro`.
pub const fn assert_is_primitive_type<T: IsPrimitiveType>() {}

#[cfg(test)]
mod tests {
    use super::{to_vec, DecodeError, Deserializer};
    use crate::de::DeserializeSeed;
    use crate::proptest::{generate_algebraic_type, generate_typed_value};
    use crate::{
        meta_type::MetaType, AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ArrayValue, Typespace, WithTypespace,
    };
    use proptest::prelude::*;
    use proptest::{collection::vec, proptest};

    #[test]
    fn decode_invalid_type_reference() {
        let ty: AlgebraicType = super::from_slice(&[0, 0, 0, 0, 0]).unwrap();
        assert_eq!(
            AlgebraicValue::decode(&ty, &mut &[0u8; 8][..]),
            Err(DecodeError::Other("Type reference &0 out of bounds".into()))
        );
    }

    fn check_invalid_reference(typespace: &Typespace, ty: &AlgebraicType, bytes: &[u8], invalid: u32) {
        let seed = WithTypespace::new(typespace, ty);
        let expected = DecodeError::Other(format!("Type reference &{invalid} out of bounds"));
        assert_eq!(seed.deserialize(Deserializer::new(&mut &*bytes)), Err(expected.clone()));
        assert_eq!(seed.validate(Deserializer::new(&mut &*bytes)), Err(expected.clone()));
        if let AlgebraicType::Array(array) = ty {
            let seed = seed.with(array);
            assert_eq!(seed.deserialize(Deserializer::new(&mut &*bytes)), Err(expected.clone()));
            assert_eq!(seed.validate(Deserializer::new(&mut &*bytes)), Err(expected));
        }
    }

    #[test]
    fn decode_and_validate_invalid_type_references() {
        for typespace in [Typespace::default(), Typespace::new(vec![AlgebraicType::U8])] {
            for invalid in [typespace.types.len() as u32, u32::MAX] {
                let ty = AlgebraicType::Ref(AlgebraicTypeRef(invalid));
                check_invalid_reference(&typespace, &ty, &[], invalid);
                check_invalid_reference(&typespace, &AlgebraicType::product([ty.clone()]), &[], invalid);
                check_invalid_reference(&typespace, &AlgebraicType::option(ty.clone()), &[0], invalid);
                for bytes in [&[0, 0, 0, 0][..], &[1, 0, 0, 0, 42][..]] {
                    check_invalid_reference(&typespace, &AlgebraicType::array(ty.clone()), bytes, invalid);
                }
            }
        }

        let ty = AlgebraicType::Ref(AlgebraicTypeRef(0));
        let typespace = Typespace::new(vec![AlgebraicType::Ref(AlgebraicTypeRef(1))]);
        check_invalid_reference(&typespace, &ty, &[], 1);
        check_invalid_reference(&typespace, &AlgebraicType::array(ty), &[0, 0, 0, 0], 1);

        // An unselected sum branch does not need to be resolved.
        check_reference_value(
            Typespace::EMPTY,
            &AlgebraicType::option(AlgebraicType::Ref(AlgebraicTypeRef(0))),
            AlgebraicValue::sum(1, AlgebraicValue::unit()),
        );
    }

    fn check_reference_value(typespace: &Typespace, ty: &AlgebraicType, value: AlgebraicValue) {
        let bytes = to_vec(&value).unwrap();
        let seed = WithTypespace::new(typespace, ty);
        let mut input = &bytes[..];
        assert_eq!(seed.deserialize(Deserializer::new(&mut input)), Ok(value));
        assert!(input.is_empty());
        let mut input = &bytes[..];
        assert_eq!(seed.validate(Deserializer::new(&mut input)), Ok(()));
        assert!(input.is_empty());
    }

    #[test]
    fn decode_and_validate_valid_type_references() {
        let ty = AlgebraicType::Ref(AlgebraicTypeRef(0));
        let typespace = Typespace::new(vec![AlgebraicType::Ref(AlgebraicTypeRef(1)), AlgebraicType::U8]);
        check_reference_value(&typespace, &ty, AlgebraicValue::U8(42));
        check_reference_value(
            &typespace,
            &AlgebraicType::array(ty),
            AlgebraicValue::Array(ArrayValue::U8(vec![42, 7].into())),
        );
    }

    #[test]
    fn decode_and_validate_recursive_type_references() {
        let ty = AlgebraicType::Ref(AlgebraicTypeRef(0));
        let typespace = Typespace::new(vec![AlgebraicType::option(ty.clone())]);
        let nil = AlgebraicValue::sum(1, AlgebraicValue::unit());
        check_reference_value(&typespace, &ty, AlgebraicValue::sum(0, AlgebraicValue::sum(0, nil)));

        let typespace = Typespace::new(vec![AlgebraicType::array(ty.clone())]);
        let empty = ArrayValue::Array(vec![].into());
        check_reference_value(
            &typespace,
            &ty,
            AlgebraicValue::Array(ArrayValue::Array(vec![empty].into())),
        );
    }

    #[test]
    fn type_to_binary_equivalent() {
        check_type(&AlgebraicType::meta_type());
    }

    #[track_caller]
    fn check_type(ty: &AlgebraicType) {
        let mut through_value = Vec::new();
        ty.as_value().encode(&mut through_value);
        let mut direct = Vec::new();
        ty.encode(&mut direct);
        assert_eq!(direct, through_value);
    }

    fn type_non_empty(ty: &AlgebraicType) -> bool {
        match ty {
            AlgebraicType::Ref(_) => unreachable!(),
            AlgebraicType::Array(elem_ty) => type_non_empty(&elem_ty.elem_ty),
            AlgebraicType::Product(elems) => elems.iter().any(|e| type_non_empty(&e.algebraic_type)),
            AlgebraicType::Sum(vars) => !vars.is_empty(),
            _ => true,
        }
    }

    proptest! {
        #[test]
        fn bsatn_enc_de_roundtrips((ty, val) in generate_typed_value()) {
            let bytes = to_vec(&val).unwrap();
            prop_assert_eq!(WithTypespace::empty(&ty).validate(Deserializer::new(&mut &bytes[..])), Ok(()));
            let val_decoded = AlgebraicValue::decode(&ty, &mut &bytes[..]).unwrap();
            prop_assert_eq!(val, val_decoded);
        }

        #[test]
        #[ignore = "This test needs to be refactored. \
                    It can generate input that allocates a lot of memory during decode, \
                    which can sometimes result in a SIGKILL. \
                    This supposedly occurs for array types with zero-width elements, \
                    but this has not been confirmed."]
        fn bsatn_invalid_wont_decode(ty in generate_algebraic_type(), bytes in vec(any::<u8>(), 0..4096)) {
            prop_assume!(type_non_empty(&ty));
            prop_assume!(WithTypespace::empty(&ty).validate(Deserializer::new(&mut &bytes[..])).is_err());
            prop_assert!(AlgebraicValue::decode(&ty, &mut &bytes[..]).is_err());
        }

        #[test]
        fn bsatn_non_zero_one_u8_aint_bool(val in 2u8..) {
            let bytes = [val];
            prop_assert_eq!(
                AlgebraicValue::decode(&AlgebraicType::Bool, &mut &bytes[..]),
                Err(DecodeError::InvalidBool(val))
            );
        }
    }
}
