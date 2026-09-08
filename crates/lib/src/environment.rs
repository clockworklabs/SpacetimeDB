//! Limits shared by the database environment store and its clients.

pub const MAX_ENV_KEY_BYTES: usize = 256;
pub const MAX_ENV_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_ENV_VARS: usize = 256;
/// Total key and literal bytes in a declaration schema, independent of runtime values.
pub const MAX_ENV_SCHEMA_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENV_UNION_ENTRIES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentValidationError {
    InvalidKey,
    ValueTooLarge,
    TooManyVariables,
}

impl std::fmt::Display for EnvironmentValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidKey => "invalid POSIX environment variable name (maximum 256 bytes)",
            Self::ValueTooLarge => "environment value exceeds 8192 UTF-8 bytes",
            Self::TooManyVariables => "environment store exceeds 256 variables",
        })
    }
}

impl std::error::Error for EnvironmentValidationError {}

pub fn validate_key(key: &str) -> Result<(), EnvironmentValidationError> {
    let bytes = key.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_ENV_KEY_BYTES
        || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        || !bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return Err(EnvironmentValidationError::InvalidKey);
    }
    Ok(())
}

/// NUL is representable in the database. Container launch separately rejects it.
pub fn validate_value(value: &str) -> Result<(), EnvironmentValidationError> {
    if value.len() > MAX_ENV_VALUE_BYTES {
        return Err(EnvironmentValidationError::ValueTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_utf8_byte_limits_and_posix_keys_without_container_policy() {
        for key in ["", "1FIRST", "A=B", "A\0B", "é", "A-B"] {
            assert_eq!(validate_key(key), Err(EnvironmentValidationError::InvalidKey));
        }
        assert!(validate_key(&"A".repeat(256)).is_ok());
        assert!(validate_key(&"A".repeat(257)).is_err());
        assert!(validate_key("SPACETIMEDB_USER_DATA").is_ok());
        assert!(validate_value("\0").is_ok());
        assert!(validate_value(&"é".repeat(4096)).is_ok());
        assert!(validate_value(&"é".repeat(4097)).is_err());
    }
}

/// Host-validated string constraints. Values remain strings in every module SDK.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, crate::SpacetimeType)]
#[sats(crate = crate)]
pub enum EnvironmentConstraint {
    AnyString,
    Literal(String),
    OneOf(Vec<String>),
}

/// Declaration metadata, never an environment value supplied during publishing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, crate::SpacetimeType)]
#[sats(crate = crate)]
pub struct EnvironmentDeclaration {
    pub name: String,
    pub constraint: EnvironmentConstraint,
    pub optional: bool,
}

/// An environment schema whose declarations have passed host validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentSchema {
    declarations: std::collections::BTreeMap<String, EnvironmentDeclaration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvironmentSchemaErrorKind {
    InvalidName,
    TooManyDeclarations,
    DuplicateDeclaration,
    EmptyUnion,
    TooManyUnionEntries,
    SchemaTooLarge,
    LiteralTooLarge,
    Undeclared,
    MissingRequired,
    ValueTooLarge,
    ConstraintMismatch,
}

/// Errors identify a key and rule, and never contain a supplied or allowed value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvironmentSchemaError {
    pub key: Option<String>,
    pub kind: EnvironmentSchemaErrorKind,
}

impl std::fmt::Display for EnvironmentSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(key) = &self.key {
            write!(f, "environment key {key:?}: ")?;
        }
        f.write_str(match self.kind {
            EnvironmentSchemaErrorKind::InvalidName => "invalid name",
            EnvironmentSchemaErrorKind::TooManyDeclarations => "too many declarations",
            EnvironmentSchemaErrorKind::DuplicateDeclaration => "duplicate declaration",
            EnvironmentSchemaErrorKind::EmptyUnion => "string union must not be empty",
            EnvironmentSchemaErrorKind::TooManyUnionEntries => "string union has too many entries",
            EnvironmentSchemaErrorKind::SchemaTooLarge => "declaration schema exceeds size limit",
            EnvironmentSchemaErrorKind::LiteralTooLarge => "declared literal exceeds value size limit",
            EnvironmentSchemaErrorKind::Undeclared => "key is not declared",
            EnvironmentSchemaErrorKind::MissingRequired => "required value is missing",
            EnvironmentSchemaErrorKind::ValueTooLarge => "value exceeds size limit",
            EnvironmentSchemaErrorKind::ConstraintMismatch => "value does not satisfy its declared string constraint",
        })
    }
}

impl std::error::Error for EnvironmentSchemaError {}

impl EnvironmentSchema {
    fn validate_metadata(declarations: &[EnvironmentDeclaration]) -> Result<(), EnvironmentSchemaError> {
        use EnvironmentSchemaErrorKind as Kind;
        if declarations.len() > MAX_ENV_VARS {
            return Err(EnvironmentSchemaError {
                key: None,
                kind: Kind::TooManyDeclarations,
            });
        }
        let mut bytes = 0usize;
        for declaration in declarations {
            // Never retain or format unvalidated key bytes in diagnostics.
            validate_key(&declaration.name).map_err(|_| EnvironmentSchemaError {
                key: None,
                kind: Kind::InvalidName,
            })?;
            let error = |kind| EnvironmentSchemaError {
                key: Some(declaration.name.clone()),
                kind,
            };
            bytes += declaration.name.len();
            if bytes > MAX_ENV_SCHEMA_BYTES {
                return Err(error(Kind::SchemaTooLarge));
            }
            let literals = match &declaration.constraint {
                EnvironmentConstraint::AnyString => &[][..],
                EnvironmentConstraint::Literal(value) => std::slice::from_ref(value),
                EnvironmentConstraint::OneOf(values) => {
                    if values.is_empty() {
                        return Err(error(Kind::EmptyUnion));
                    }
                    if values.len() > MAX_ENV_UNION_ENTRIES {
                        return Err(error(Kind::TooManyUnionEntries));
                    }
                    values.as_slice()
                }
            };
            for value in literals {
                validate_value(value).map_err(|_| error(Kind::LiteralTooLarge))?;
                bytes += value.len();
                if bytes > MAX_ENV_SCHEMA_BYTES {
                    return Err(error(Kind::SchemaTooLarge));
                }
            }
        }
        Ok(())
    }

    pub fn new(declarations: Vec<EnvironmentDeclaration>) -> Result<Self, EnvironmentSchemaError> {
        Self::validate_metadata(&declarations)?;
        let mut schema = Self::default();
        for mut declaration in declarations {
            if let EnvironmentConstraint::OneOf(values) = &mut declaration.constraint {
                values.sort_unstable();
                values.dedup();
            }
            if schema.declarations.contains_key(&declaration.name) {
                return Err(EnvironmentSchemaError {
                    key: Some(declaration.name),
                    kind: EnvironmentSchemaErrorKind::DuplicateDeclaration,
                });
            }
            schema.declarations.insert(declaration.name.clone(), declaration);
        }
        Ok(schema)
    }

    /// Check bounds before cloning raw untrusted metadata into the validated schema.
    pub fn from_declarations(declarations: &[EnvironmentDeclaration]) -> Result<Self, EnvironmentSchemaError> {
        Self::validate_metadata(declarations)?;
        Self::new(declarations.to_vec())
    }

    pub fn get(&self, name: &str) -> Option<&EnvironmentDeclaration> {
        self.declarations.get(name)
    }

    pub fn declarations(&self) -> impl ExactSizeIterator<Item = &EnvironmentDeclaration> {
        self.declarations.values()
    }

    pub fn into_declarations(self) -> Vec<EnvironmentDeclaration> {
        self.declarations.into_values().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// Validate a complete publish input. Existing stored values are not inputs.
    pub fn validate_values(
        &self,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), EnvironmentSchemaError> {
        use EnvironmentSchemaErrorKind as Kind;
        for (name, value) in values {
            validate_key(name).map_err(|_| EnvironmentSchemaError {
                key: None,
                kind: Kind::InvalidName,
            })?;
            let error = |kind| EnvironmentSchemaError {
                key: Some(name.clone()),
                kind,
            };
            let declaration = self.get(name).ok_or_else(|| error(Kind::Undeclared))?;
            validate_value(value).map_err(|_| error(Kind::ValueTooLarge))?;
            let matches = match &declaration.constraint {
                EnvironmentConstraint::AnyString => true,
                EnvironmentConstraint::Literal(expected) => value == expected,
                EnvironmentConstraint::OneOf(allowed) => allowed.binary_search(value).is_ok(),
            };
            if !matches {
                return Err(error(Kind::ConstraintMismatch));
            }
        }
        for declaration in self.declarations() {
            if !declaration.optional && !values.contains_key(&declaration.name) {
                return Err(EnvironmentSchemaError {
                    key: Some(declaration.name.clone()),
                    kind: Kind::MissingRequired,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn declaration(name: &str, constraint: EnvironmentConstraint, optional: bool) -> EnvironmentDeclaration {
        EnvironmentDeclaration {
            name: name.into(),
            constraint,
            optional,
        }
    }

    #[test]
    fn complete_inputs_preserve_optional_empty_and_exact_string_constraints() {
        let schema = EnvironmentSchema::new(vec![
            declaration("REQUIRED", EnvironmentConstraint::AnyString, false),
            declaration(
                "MODE",
                EnvironmentConstraint::OneOf(vec!["false".into(), "true".into(), "false".into()]),
                false,
            ),
            declaration("OPTIONAL", EnvironmentConstraint::Literal("".into()), true),
        ])
        .unwrap();
        let mut values = BTreeMap::from([("REQUIRED".into(), "\0雪".into()), ("MODE".into(), "false".into())]);
        schema.validate_values(&values).unwrap();
        values.insert("OPTIONAL".into(), "".into());
        schema.validate_values(&values).unwrap();
        values.insert("MODE".into(), "False".into());
        assert_eq!(
            schema.validate_values(&values).unwrap_err().kind,
            EnvironmentSchemaErrorKind::ConstraintMismatch
        );
        values.remove("MODE");
        assert_eq!(
            schema.validate_values(&values).unwrap_err().kind,
            EnvironmentSchemaErrorKind::MissingRequired
        );
        values.insert("UNDECLARED".into(), "secret-marker".into());
        let error = schema.validate_values(&values).unwrap_err();
        assert_eq!(error.kind, EnvironmentSchemaErrorKind::Undeclared);
        assert!(!format!("{error:?}: {error}").contains("secret-marker"));
    }

    #[test]
    fn declaration_limits_count_absent_optionals_and_reject_invalid_metadata() {
        let optional = declaration("A", EnvironmentConstraint::AnyString, true);
        assert_eq!(
            EnvironmentSchema::new(vec![optional.clone(), optional])
                .unwrap_err()
                .kind,
            EnvironmentSchemaErrorKind::DuplicateDeclaration
        );
        assert_eq!(
            EnvironmentSchema::new(
                (0..=MAX_ENV_VARS)
                    .map(|i| declaration(&format!("K{i}"), EnvironmentConstraint::AnyString, true))
                    .collect()
            )
            .unwrap_err()
            .kind,
            EnvironmentSchemaErrorKind::TooManyDeclarations
        );
        for (name, constraint, expected) in [
            (
                "A-B",
                EnvironmentConstraint::AnyString,
                EnvironmentSchemaErrorKind::InvalidName,
            ),
            (
                "A",
                EnvironmentConstraint::OneOf(vec![]),
                EnvironmentSchemaErrorKind::EmptyUnion,
            ),
            (
                "A",
                EnvironmentConstraint::Literal("x".repeat(MAX_ENV_VALUE_BYTES + 1)),
                EnvironmentSchemaErrorKind::LiteralTooLarge,
            ),
        ] {
            assert_eq!(
                EnvironmentSchema::new(vec![declaration(name, constraint, true)])
                    .unwrap_err()
                    .kind,
                expected
            );
        }
        assert!(EnvironmentSchema::default().validate_values(&BTreeMap::new()).is_ok());
        assert_eq!(
            EnvironmentSchema::default()
                .validate_values(&BTreeMap::from([("A".into(), "".into())]))
                .unwrap_err()
                .kind,
            EnvironmentSchemaErrorKind::Undeclared
        );
    }

    #[test]
    fn raw_metadata_is_bounded_before_copying_or_formatting_untrusted_keys() {
        let invalid = format!("private-marker\n{}", "x".repeat(100_000));
        let error =
            EnvironmentSchema::new(vec![declaration(&invalid, EnvironmentConstraint::AnyString, true)]).unwrap_err();
        assert_eq!(error.key, None);
        assert!(!format!("{error:?}: {error}").contains("private-marker"));
        let error = EnvironmentSchema::new(vec![declaration(
            "A",
            EnvironmentConstraint::OneOf(vec!["".into(); MAX_ENV_UNION_ENTRIES + 1]),
            true,
        )])
        .unwrap_err();
        assert_eq!(error.kind, EnvironmentSchemaErrorKind::TooManyUnionEntries);
        let error = EnvironmentSchema::new(vec![declaration(
            "A",
            EnvironmentConstraint::OneOf(vec!["x".repeat(MAX_ENV_VALUE_BYTES); MAX_ENV_UNION_ENTRIES]),
            true,
        )])
        .unwrap_err();
        assert_eq!(error.kind, EnvironmentSchemaErrorKind::SchemaTooLarge);
    }
}
