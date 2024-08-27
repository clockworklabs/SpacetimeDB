//! Validation code for various versions of ModuleDef.

use crate::error::ValidationErrors;

pub mod v8;
pub mod v9;

pub type Result<T> = std::result::Result<T, ValidationErrors>;

/// Helpers used in tests for validation modules.
#[cfg(test)]
pub mod tests {
    use itertools::Itertools;
    use spacetimedb_lib::{db::raw_def::v9::RawScopedTypeNameV9, AlgebraicType};
    use spacetimedb_sats::{Typespace, WithTypespace};

    use crate::{def::ScopedTypeName, identifier::Identifier};

    /// Create an identifier, panicking if invalid.
    pub fn expect_identifier(data: impl Into<Box<str>>) -> Identifier {
        Identifier::new(data.into()).expect("invalid identifier")
    }

    /// Expect a name in the form "(scope::)*name".
    /// Panics if the input is invalid.
    pub fn expect_type_name(scoped_name: &str) -> ScopedTypeName {
        let mut scope = scoped_name
            .split("::")
            .map(|module| {
                Identifier::new(module.into()).expect("all components of a scoped name must be valid identifiers.")
            })
            .collect::<Vec<_>>();
        let name = scope.pop().expect("scoped names must contain at least one identifier");
        let scope = scope.into();

        ScopedTypeName { name, scope }
    }

    /// Expect a name in the form "(scope::)*name".
    /// Panics if the input is invalid.
    pub fn expect_raw_type_name(scoped_name: &str) -> RawScopedTypeNameV9 {
        let mut scope = scoped_name.split("::").map_into().collect::<Vec<_>>();
        let name = scope.pop().expect("scoped names must contain at least one identifier");
        let scope = scope.into();

        RawScopedTypeNameV9 { name, scope }
    }

    /// Resolve a type in a typespace, expecting success.
    pub fn expect_resolve(typespace: &Typespace, ty: &AlgebraicType) -> AlgebraicType {
        WithTypespace::new(typespace, ty)
            .resolve_refs()
            .expect("failed to resolve type")
    }

    #[test]
    fn test_expect_type_name() {
        assert_eq!(
            expect_raw_type_name("foo::bar::baz"),
            RawScopedTypeNameV9 {
                scope: Box::new(["foo".into(), "bar".into()]),
                name: "baz".into(),
            }
        );
        assert_eq!(
            expect_raw_type_name("foo"),
            RawScopedTypeNameV9 {
                scope: Default::default(),
                name: "foo".into(),
            }
        );
        assert_eq!(
            expect_type_name("foo::bar::baz"),
            ScopedTypeName {
                scope: Box::new([expect_identifier("foo"), expect_identifier("bar")]),
                name: expect_identifier("baz"),
            }
        );
        assert_eq!(
            expect_type_name("foo"),
            ScopedTypeName {
                name: expect_identifier("foo"),
                scope: Default::default()
            }
        );
    }
}
