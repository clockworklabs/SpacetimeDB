//! Helpers to allow deserializing data using a ReducerDef.

use crate::def::ReducerDef;
use spacetimedb_lib::sats::{self, de, ser, ProductValue};
use spacetimedb_sats::impl_serialize;

/// Wrapper around a `ReducerDef` that allows deserializing to a `ProductValue` at the type
/// of the reducer's parameter `ProductType`.
#[derive(Clone, Copy)]
pub struct ReducerArgsDeserializeSeed<'a>(pub sats::WithTypespace<'a, ReducerDef>);

impl<'a> ReducerArgsDeserializeSeed<'a> {
    /// Get the reducer def of this seed.
    pub fn reducer_def(&self) -> &'a ReducerDef {
        self.0.ty()
    }
}

impl<'de> de::DeserializeSeed<'de> for ReducerArgsDeserializeSeed<'_> {
    type Output = ProductValue;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de> de::ProductVisitor<'de> for ReducerArgsDeserializeSeed<'_> {
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
