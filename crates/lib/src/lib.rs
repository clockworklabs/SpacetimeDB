pub use spacetimedb_sats::buffer;
pub mod data_key;
pub use spacetimedb_sats::de;
pub mod error;
pub mod hash;
#[cfg(feature = "serde")]
pub mod name;
pub mod primary_key;
pub use spacetimedb_sats::ser;
pub mod type_def {
    pub use spacetimedb_sats::{
        AlgebraicType as TypeDef, ProductType as TupleDef, ProductTypeElement as ElementDef, SumType as EnumDef,
    };
}
pub mod type_value {
    pub use spacetimedb_sats::{AlgebraicValue as TypeValue, ProductValue as TupleValue};
}
#[cfg(feature = "serde")]
pub mod recovery;
pub mod version;
pub use spacetimedb_sats::bsatn;

pub use data_key::DataKey;
pub use hash::Hash;
pub use primary_key::PrimaryKey;
pub use type_def::*;
pub use type_value::{TupleValue, TypeValue};

pub use spacetimedb_sats as sats;

pub const SCHEMA_FORMAT_VERSION: u16 = 1;

#[macro_export]
macro_rules! abi_versions {
    ($mac:ident) => {
        $mac! {
            V0 => (0, 0),
            V0_3_3 => (1, 1),
        }
    };
}

extern crate self as spacetimedb_lib;

#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub struct TableDef {
    pub name: String,
    pub data: sats::AlgebraicTypeRef,
    /// must be sorted!
    pub unique_columns: Vec<u8>,
}

#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub struct ReducerDef {
    pub name: Option<Box<str>>,
    pub args: Vec<ElementDef>,
}

impl ReducerDef {
    pub fn encode(&self, writer: &mut impl buffer::BufWriter) {
        bsatn::to_writer(writer, self).unwrap()
    }

    pub fn serialize_args<'a>(ty: sats::TypeInSpace<'a, Self>, value: &'a TupleValue) -> impl ser::Serialize + 'a {
        ReducerArgsWithSchema { value, ty }
    }

    pub fn deserialize(
        ty: sats::TypeInSpace<'_, Self>,
    ) -> impl for<'de> de::DeserializeSeed<'de, Output = TupleValue> + '_ {
        ReducerDeserialize(ty)
    }
}

struct ReducerDeserialize<'a>(sats::TypeInSpace<'a, ReducerDef>);

impl<'de> de::DeserializeSeed<'de> for ReducerDeserialize<'_> {
    type Output = TupleValue;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de> de::ProductVisitor<'de> for ReducerDeserialize<'_> {
    type Output = TupleValue;

    fn product_name(&self) -> Option<&str> {
        self.0.ty().name.as_deref()
    }
    fn product_len(&self) -> usize {
        self.0.ty().args.len()
    }
    fn product_kind(&self) -> de::ProductKind {
        de::ProductKind::ReducerArgs
    }

    fn visit_seq_product<A: de::SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_seq_product(self.0.map(|r| &*r.args), &self, tup)
    }

    fn visit_named_product<A: de::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_named_product(self.0.map(|r| &*r.args), &self, tup)
    }
}

struct ReducerArgsWithSchema<'a> {
    value: &'a TupleValue,
    ty: sats::TypeInSpace<'a, ReducerDef>,
}

impl ser::Serialize for ReducerArgsWithSchema<'_> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use itertools::Itertools;
        use ser::SerializeSeqProduct;
        let mut seq = serializer.serialize_seq_product(self.value.elements.len())?;
        for (value, elem) in self.value.elements.iter().zip_eq(&self.ty.ty().args) {
            seq.serialize_element(&self.ty.with(&elem.algebraic_type).with_value(value))?;
        }
        seq.end()
    }
}

#[derive(Debug, Clone, enum_as_inner::EnumAsInner)]
pub enum EntityDef {
    Table(TableDef),
    Reducer(ReducerDef),
}

#[derive(Debug, Clone)]
pub enum ModuleItemDef {
    Entity(EntityDef),
    TypeAlias(sats::AlgebraicTypeRef),
}

// use std::fmt;
//
// #[cfg(feature = "serde")]
// use serde::de::Expected as SerdeExpected;
// #[cfg(not(feature = "serde"))]
// use Sized as SerdeExpected;
// fn fmt_fn(f: impl Fn(&mut fmt::Formatter) -> fmt::Result) -> impl fmt::Display + fmt::Debug + SerdeExpected {
//     struct FDisplay<F>(F);
//     impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for FDisplay<F> {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             (self.0)(f)
//         }
//     }
//     impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Debug for FDisplay<F> {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             (self.0)(f)
//         }
//     }
//     #[cfg(feature = "serde")]
//     impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> serde::de::Expected for FDisplay<F> {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             (self.0)(f)
//         }
//     }
//     FDisplay(f)
// }
