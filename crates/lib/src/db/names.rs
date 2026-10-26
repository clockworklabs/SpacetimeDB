use super::raw_def::v10::CaseConversionPolicy;
use crate::sats::Typespace;
use crate::AlgebraicType;
use convert_case::{Case, Casing};

/// Apply the module's case conversion policy to a source identifier.
pub fn canonical_name(name: &str, policy: CaseConversionPolicy) -> String {
    match policy {
        CaseConversionPolicy::None => name.to_owned(),
        CaseConversionPolicy::SnakeCase => name.to_case(Case::Snake),
    }
}

/// Apply module naming rules to product fields and sum variants in a typespace.
pub fn canonicalize_typespace(policy: CaseConversionPolicy, typespace: &mut Typespace) {
    for ty in &mut typespace.types {
        canonicalize_type(policy, ty);
    }
}

fn canonicalize_type(policy: CaseConversionPolicy, ty: &mut AlgebraicType) {
    if ty.is_special() {
        return;
    }
    match ty {
        AlgebraicType::Product(product) => {
            for element in &mut product.elements {
                if let Some(name) = &mut element.name {
                    *name = canonical_name(name.as_ref(), policy).into();
                }
                canonicalize_type(policy, &mut element.algebraic_type);
            }
        }
        AlgebraicType::Sum(sum) => {
            for variant in &mut sum.variants {
                if let Some(name) = &mut variant.name
                    && matches!(policy, CaseConversionPolicy::SnakeCase)
                {
                    *name = name.to_string().to_case(Case::Camel).into();
                }
                canonicalize_type(policy, &mut variant.algebraic_type);
            }
        }
        AlgebraicType::Array(array) => canonicalize_type(policy, &mut array.elem_ty),
        _ => {}
    }
}
