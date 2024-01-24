use crate::buffer::{BufReader, BufWriter, CountWriter};
use crate::de::{BasicSmallVecVisitor, Deserialize, DeserializeSeed, Deserializer as _};
use crate::ser::Serialize;
use crate::Typespace;
use smallvec::SmallVec;

pub mod de;
pub mod ser;

pub use de::Deserializer;
pub use ser::Serializer;

pub use crate::buffer::DecodeError;

/// Serialize `value` into the buffered writer `w` in the BSATN format.
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
pub fn from_reader<'de, T: Deserialize<'de>>(reader: &mut impl BufReader<'de>) -> Result<T, DecodeError> {
    T::deserialize(Deserializer::new(reader))
}

/// Deserialize a `T` from the BSATN format in `bytes`.
pub fn from_slice<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, DecodeError> {
    from_reader(&mut &*bytes)
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
                crate::WithTypespace::new(&Typespace::new(Vec::new()), ty).deserialize(Deserializer::new(bytes))
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
