//! Helpers to allow deserializing data using a ReducerDef.

use crate::def::{ProcedureDef, ReducerDef};
use spacetimedb_lib::{
    sats::{self, de, ser, ProductValue},
    ProductType,
};
use spacetimedb_sats::impl_serialize;

pub trait ArgsSeed: for<'de> de::DeserializeSeed<'de, Output = ProductValue> {
    fn params(&self) -> &ProductType;
}

/// Define `struct_name` as a newtype wrapper around [`WithTypespace`] of `inner_ty`,
/// and implement [`de::DeserializeSeed`] and [`de::ProductVisitor`] for that newtype.
///
/// `ReducerArgs` (defined in the spacetimedb_core crate) will use this type
/// to deserialize the arguments to a reducer or procedure
/// at the appropriate type for that specific function, which is known only at runtime.
macro_rules! define_args_deserialize_seed {
    ($struct_vis:vis struct $struct_name:ident($field_vis:vis $inner_ty:ty)) => {
        #[doc = concat!(
            "Wrapper around a [`",
            stringify!($inner_ty),
            "`] that allows deserializing to a [`ProductValue`] at the type of the def's parameter `ProductType`."
        )]
        #[derive(Clone, Copy)]
        $struct_vis struct $struct_name<'a>($field_vis sats::WithTypespace<'a, $inner_ty> );

        impl<'a> $struct_name<'a> {
            #[doc = concat!(
                "Get the inner [`",
                stringify!($inner_ty),
                "`] of this seed."
            )]
            $struct_vis fn inner_def(&self) -> &'a $inner_ty {
                self.0.ty()
            }
        }

        impl<'de> de::DeserializeSeed<'de> for $struct_name<'_> {
            type Output = ProductValue;

            fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
                deserializer.deserialize_product(self)
            }
        }

        impl<'de> de::ProductVisitor<'de> for $struct_name<'_> {
            type Output = ProductValue;

            fn product_name(&self) -> Option<&str> {
                Some(&self.0.ty().name)
            }
            fn product_len(&self) -> usize {
                self.0.ty().params.elements.len()
            }
            fn product_kind(&self) -> de::ProductKind {
                de::ProductKind::ReducerArgs
            }

            fn visit_seq_product<A: de::SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
                de::visit_seq_product(self.0.map(|r| &*r.params.elements), &self, tup)
            }

            fn visit_named_product<A: de::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
                de::visit_named_product(self.0.map(|r| &*r.params.elements), &self, tup)
            }
        }

        impl<'a> ArgsSeed for $struct_name<'a> {
            fn params(&self) -> &ProductType {
                &self.0.ty().params
            }
        }
    }
}

define_args_deserialize_seed!(pub struct ReducerArgsDeserializeSeed(pub ReducerDef));
define_args_deserialize_seed!(pub struct ProcedureArgsDeserializeSeed(pub ProcedureDef));

pub struct ReducerArgsWithSchema<'a> {
    value: &'a ProductValue,
    ty: sats::WithTypespace<'a, ReducerDef>,
}
impl_serialize!([] ReducerArgsWithSchema<'_>, (self, ser) => {
    use itertools::Itertools;
    use ser::SerializeSeqProduct;
    let mut seq = ser.serialize_seq_product(self.value.elements.len())?;
    for (value, elem) in self.value.elements.iter().zip_eq(&*self.ty.ty().params.elements) {
        seq.serialize_element(&self.ty.with(&elem.algebraic_type).with_value(value))?;
    }
    seq.end()
});
