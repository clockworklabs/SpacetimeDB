use crate::buffer::{BufReader, BufWriter};
use crate::de::{Deserialize, DeserializeSeed};
use crate::ser::Serialize;
use crate::Typespace;

pub mod de;
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

/// Deserialize a `T` from the BSATM format in the buffered `reader`.
pub fn from_reader<'de, T: Deserialize<'de>>(reader: &mut impl BufReader<'de>) -> Result<T, DecodeError> {
    T::deserialize(Deserializer::new(reader))
}

/// Deserialize a `T` from the BSATM format in `bytes`.
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
            pub fn decode<'a>(
                algebraic_type: &<Self as crate::Value>::Type,
                bytes: &mut impl BufReader<'a>,
            ) -> Result<Self, DecodeError> {
                crate::WithTypespace::new(&Typespace::new(Vec::new()), algebraic_type)
                    .deserialize(Deserializer::new(bytes))
            }

            pub fn encode(&self, bytes: &mut impl BufWriter) {
                to_writer(bytes, self).unwrap()
            }
        }
    };
}

codec_funcs!(crate::AlgebraicType);
codec_funcs!(crate::BuiltinType);
codec_funcs!(crate::ProductType);
codec_funcs!(crate::SumType);
codec_funcs!(crate::ProductTypeElement);
codec_funcs!(crate::SumTypeVariant);

codec_funcs!(val: crate::AlgebraicValue);
codec_funcs!(val: crate::ProductValue);
codec_funcs!(val: crate::SumValue);
codec_funcs!(val: crate::BuiltinValue);
