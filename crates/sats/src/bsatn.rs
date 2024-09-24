use crate::buffer::{BufReader, BufWriter, CountWriter};
use crate::de::{BasicSmallVecVisitor, Deserialize, DeserializeSeed, Deserializer as _};
use crate::ser::Serialize;
use crate::{ProductValue, Typespace, WithTypespace};
use ser::BsatnError;
use smallvec::SmallVec;

pub mod de;
pub mod eq;
pub mod ser;

pub use de::Deserializer;
pub use ser::Serializer;

pub use crate::buffer::DecodeError;

/// Serialize `value` into the buffered writer `w` in the BSATN format.
pub fn to_writer<W: BufWriter, T: Serialize + ?Sized>(w: &mut W, value: &T) -> Result<(), ser::BsatnError> {
    value.serialize(Serializer::new(w))
}

/// Serialize `value` into a `Vec<u8>` in the BSATN format.
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, ser::BsatnError> {
    let mut v = Vec::new();
    to_writer(&mut v, value)?;
    Ok(v)
}

/// Computes the size of `val` when BSATN encoding without actually encoding.
pub fn to_len<T: Serialize + ?Sized>(value: &T) -> Result<usize, ser::BsatnError> {
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

/// Types that can be encoded to BSATN.
///
/// Implementations of this trait may be more efficient than directly calling [`bsatn::to_vec`].
/// In particular, for [`RowRef`], this method will use a [`StaticBsatnLayout`] if one is available,
/// avoiding expensive runtime type dispatch.
pub trait ToBsatn {
    /// BSATN-encode the row referred to by `self` into a freshly-allocated `Vec<u8>`.
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError>;

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf`, similar to [`Vec::extend`].
    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError>;

    /// Returns the static size of the type of this object.
    ///
    /// When this returns `Some(_)` there is also a `StaticBsatnLayout`.
    fn static_bsatn_size(&self) -> Option<u16>;
}

impl<T: ToBsatn> ToBsatn for &T {
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        T::to_bsatn_vec(*self)
    }
    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
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

    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        to_writer(buf, self)
    }

    fn static_bsatn_size(&self) -> Option<u16> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::to_vec;
    use crate::proptest::generate_typed_value;
    use crate::{meta_type::MetaType, AlgebraicType, AlgebraicValue};
    use proptest::prelude::*;
    use proptest::proptest;

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

    proptest! {
        #[test]
        fn bsatn_enc_de_roundtrips((ty, val) in generate_typed_value()) {
            let bytes = to_vec(&val).unwrap();
            let val_decoded = AlgebraicValue::decode(&ty, &mut &bytes[..]).unwrap();
            prop_assert_eq!(val, val_decoded);
        }
    }
}
