use convert_case::{Case, Casing};
use spacetimedb_lib::db::raw_def::v10::CaseConversionPolicy;
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::{AlgebraicType, ProductType, Typespace};

/// Apply the module's case conversion policy to a source identifier.
pub fn canonical_name(name: &str, policy: CaseConversionPolicy) -> String {
    match policy {
        CaseConversionPolicy::None => name.to_owned(),
        CaseConversionPolicy::SnakeCase => name.to_case(Case::Snake),
        _ => unreachable!("unsupported case conversion policy"),
    }
}

/// Resolve a schema name, preserving an explicit override verbatim.
///
/// Tables, functions, and namespaces use the same precedence rule. Applying case
/// conversion to an explicit name would change the registered name.
pub fn canonical_name_with_override(
    source_name: &str,
    explicit_name: Option<&RawIdentifier>,
    policy: CaseConversionPolicy,
) -> RawIdentifier {
    explicit_name
        .cloned()
        .unwrap_or_else(|| canonical_name(source_name, policy).into())
}

/// Resolve parameter names without changing their types or visiting inline field names.
///
/// Keep missing names as `None` so validation can report its usual error with the
/// source location. Local outboxes hash the resulting validated reducer parameters.
pub fn canonical_parameter_names(
    params: &ProductType,
    policy: CaseConversionPolicy,
) -> impl Iterator<Item = Option<RawIdentifier>> + '_ {
    params.elements.iter().map(move |param| {
        param
            .name
            .as_ref()
            .map(|name| canonical_name(name.as_ref(), policy).into())
    })
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
