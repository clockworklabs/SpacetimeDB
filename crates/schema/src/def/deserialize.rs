//! Helpers to allow deserializing data using a ReducerDef.

use crate::{
    def::{ProcedureDef, ReducerDef, ViewDef},
    identifier::Identifier,
    type_for_generate::ProductTypeDef,
};
use spacetimedb_lib::{
    sats::{self, de, impl_serialize, ser, ProductValue},
    ProductType,
};

/// Wrapper around a function def that allows deserializing to a [`ProductValue`] at the type of the def's parameter [`ProductType`].
///
/// Sensible instantiations for `Def` are [`ProcedureDef`], [`ReducerDef`] and [`ViewDef`].
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
    fn params_for_generate(&self) -> &ProductTypeDef;
    fn name(&self) -> &Identifier;
}

impl FunctionDef for ReducerDef {
    fn params_for_generate(&self) -> &ProductTypeDef {
        &self.params_for_generate
    }
    fn params(&self) -> &ProductType {
        &self.params
    }
    fn name(&self) -> &Identifier {
        self.name.local()
    }
}

impl FunctionDef for ProcedureDef {
    fn params_for_generate(&self) -> &ProductTypeDef {
        &self.params_for_generate
    }
    fn params(&self) -> &ProductType {
        &self.params
    }
    fn name(&self) -> &Identifier {
        &self.name
    }
}

impl FunctionDef for ViewDef {
    fn params_for_generate(&self) -> &ProductTypeDef {
        &self.params_for_generate
    }
    fn params(&self) -> &ProductType {
        &self.params
    }
    fn name(&self) -> &Identifier {
        &self.name
    }
}

impl<Def: FunctionDef> ArgsSeed<'_, Def> {
    pub fn name(&self) -> &Identifier {
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
        de::visit_named_product_with_field_names(self.0.map(|r| &*r.params().elements), &self, tup, self)
    }
}

impl<'de, Def: FunctionDef> de::FieldNameVisitor<'de> for ArgsSeed<'_, Def> {
    type Output = usize;

    fn kind(&self) -> de::ProductKind {
        de::ProductKind::ReducerArgs
    }

    fn field_names(&self) -> impl '_ + Iterator<Item = Option<&str>> {
        self.params().elements.iter().map(|param| param.name.as_deref()).chain(
            self.0
                .ty()
                .params_for_generate()
                .elements
                .iter()
                .filter(|(name, _)| !self.params().elements.iter().any(|param| param.has_name(name)))
                .map(|(name, _)| Some(name.as_ref())),
        )
    }

    fn visit<E: de::Error>(self, name: &str) -> Result<Self::Output, E> {
        // Preserve existing requests when a canonical alias collides with another
        // parameter's source name. Both spellings otherwise identify the same slot,
        // so the shared product decoder also rejects duplicate aliases.
        self.params()
            .elements
            .iter()
            .position(|param| param.has_name(name))
            .or_else(|| {
                self.0
                    .ty()
                    .params_for_generate()
                    .elements
                    .iter()
                    .position(|(canonical, _)| canonical == name)
            })
            .ok_or_else(|| E::unknown_field_name(name, &self))
    }

    fn visit_seq(self, index: usize) -> Self::Output {
        assert!(index < self.params().elements.len());
        index
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::ModuleDef;
    use sats::{de::DeserializeSeed, product};
    use spacetimedb_lib::{
        db::raw_def::v10::{CaseConversionPolicy, RawModuleDefV10Builder},
        AlgebraicType,
    };

    fn decode(module: &ModuleDef, def: &impl FunctionDef, json: &str) -> Result<ProductValue, String> {
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let value = module
            .arg_seed_for(def)
            .deserialize(de::serde::SerdeDeserializer::new(&mut deserializer))
            .map_err(|error| error.0.to_string())?;
        deserializer.end().map_err(|error| error.to_string())?;
        Ok(value)
    }

    fn check_parameter_aliases(module: &ModuleDef, def: &impl FunctionDef, policy: CaseConversionPolicy) {
        let expected = product![42u64, "request-7"];
        assert_eq!(decode(module, def, r#"[42,"request-7"]"#).unwrap(), expected);
        assert_eq!(
            decode(module, def, r#"{"RequestID":"request-7","MessageBody":42}"#).unwrap(),
            expected
        );
        if matches!(policy, CaseConversionPolicy::SnakeCase) {
            for json in [
                r#"{"message_body":42,"request_id":"request-7"}"#,
                r#"{"RequestID":"request-7","message_body":42}"#,
            ] {
                assert_eq!(decode(module, def, json).unwrap(), expected);
            }
            for json in [
                r#"{"MessageBody":42,"message_body":43}"#,
                r#"{"message_body":42,"MessageBody":43}"#,
            ] {
                assert!(decode(module, def, json).unwrap_err().contains("duplicate"));
            }
        } else {
            assert!(decode(module, def, r#"{"message_body":42,"RequestID":"request-7"}"#).is_err());
        }
        assert!(decode(module, def, r#"{"MessageBody":42}"#)
            .unwrap_err()
            .contains("missing"));
        assert!(decode(module, def, r#"{"unknown":42}"#)
            .unwrap_err()
            .contains("unknown"));
    }

    #[test]
    fn named_json_parameter_aliases_are_shared_by_functions() {
        for policy in [CaseConversionPolicy::None, CaseConversionPolicy::SnakeCase] {
            let mut builder = RawModuleDefV10Builder::new();
            builder.set_case_conversion_policy(policy);
            let params = ProductType::from([
                ("MessageBody", AlgebraicType::U64),
                ("RequestID", AlgebraicType::String),
            ]);
            builder.add_reducer("receive", params.clone());
            builder.add_procedure("read_message", params.clone(), AlgebraicType::U64);
            let row = builder.add_algebraic_type(
                [],
                "Message",
                AlgebraicType::product([("value", AlgebraicType::U64)]),
                false,
            );
            builder.add_view(
                "messages",
                0,
                true,
                true,
                params,
                AlgebraicType::array(AlgebraicType::Ref(row)),
            );
            let module: ModuleDef = builder.finish().try_into().unwrap();
            check_parameter_aliases(&module, module.reducer_by_name("receive").unwrap().1, policy);
            check_parameter_aliases(&module, module.procedures().next().unwrap(), policy);
            check_parameter_aliases(&module, module.views().next().unwrap(), policy);
        }
    }

    #[test]
    fn named_json_original_parameter_names_take_precedence_over_aliases() {
        let mut builder = RawModuleDefV10Builder::new();
        builder.set_case_conversion_policy(CaseConversionPolicy::SnakeCase);
        builder.add_reducer(
            "receive",
            ProductType::from([("fooBar", AlgebraicType::U64), ("foo_bar", AlgebraicType::String)]),
        );
        let module: ModuleDef = builder.finish().try_into().unwrap();
        let reducer = module.reducer_by_name("receive").unwrap().1;
        assert_eq!(
            decode(&module, reducer, r#"{"foo_bar":"original","fooBar":42}"#).unwrap(),
            product![42u64, "original"]
        );
    }
}
