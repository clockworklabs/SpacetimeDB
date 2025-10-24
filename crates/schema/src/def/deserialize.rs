//! Helpers to allow deserializing data using a ReducerDef.

use crate::def::{ProcedureDef, ReducerDef};
use spacetimedb_lib::{
    sats::{self, de, impl_serialize, ser, ProductValue},
    ProductType,
};

/// Wrapper around a function def that allows deserializing to a [`ProductValue`] at the type of the def's parameter [`ProductType`].
///
/// Sensible instantiations for `Def` are [`ProcedureDef`] and [`ReducerDef`].
pub struct ArgsSeed<'a, Def>(pub sats::WithTypespace<'a, Def>);

// Manual impls of traits rather than derives,
// 'cause derives are always constrained on all type parameters,
// even though `ArgsSeed<Def: ?Copy>: Copy` in our case.
impl<Def> Clone for ArgsSeed<'_, Def> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Def> Copy for ArgsSeed<'_, Def> {}

pub trait FunctionDef {
    fn params(&self) -> &ProductType;
    fn name(&self) -> &str;
}

impl FunctionDef for ReducerDef {
    fn params(&self) -> &ProductType {
        &self.params
    }
    fn name(&self) -> &str {
        &self.name
    }
}

impl FunctionDef for ProcedureDef {
    fn params(&self) -> &ProductType {
        &self.params
    }
    fn name(&self) -> &str {
        &self.name
    }
}

impl<Def: FunctionDef> ArgsSeed<'_, Def> {
    pub fn name(&self) -> &str {
        self.0.ty().name()
    }

    pub fn params(&self) -> &ProductType {
        self.0.ty().params()
    }
}

impl<'de, Def: FunctionDef> de::DeserializeSeed<'de> for ArgsSeed<'_, Def> {
    type Output = ProductValue;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de, Def: FunctionDef> de::ProductVisitor<'de> for ArgsSeed<'_, Def> {
    type Output = ProductValue;

    fn product_name(&self) -> Option<&str> {
        Some(self.0.ty().name())
    }

    fn product_len(&self) -> usize {
        self.0.ty().params().elements.len()
    }

    fn product_kind(&self) -> de::ProductKind {
        de::ProductKind::ReducerArgs
    }

    fn visit_seq_product<A: de::SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_seq_product(self.0.map(|r| &*r.params().elements), &self, tup)
    }

    fn visit_named_product<A: de::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_named_product(self.0.map(|r| &*r.params().elements), &self, tup)
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
