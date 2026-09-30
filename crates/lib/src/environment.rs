//! Limits shared by the database environment store and its clients.

use crate::db::raw_def::v10::{RawEnvVarTypeV10, RawEnvironmentDeclarationV10};

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
            Self::ValueTooLarge => "environment value too large (maximum 8192 bytes)",
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

/// Host-validated types for environment values, which are stored as strings.
// TODO: Consider representing these types with a subset of SATS if
// AlgebraicType gains StringLiteral(String) and Union(Vec<AlgebraicType>).
// For example, Union([StringLiteral("development"), StringLiteral("production")])
// would accept either string directly, without the runtime tag used by SATS Sum.
// Changing this union payload to recursive types requires ABI compatibility handling.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, crate::SpacetimeType)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[sats(crate = crate)]
pub enum EnvVarType {
    String,
    StringLiteral(String),
    /// An untagged union of string literals, stored as their literal values.
    Union(Vec<String>),
}

/// Declaration metadata, never an environment value supplied during publishing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, crate::SpacetimeType)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[sats(crate = crate)]
pub struct EnvironmentDeclaration {
    pub name: String,
    pub ty: EnvVarType,
    pub optional: bool,
}

impl From<EnvironmentDeclaration> for RawEnvironmentDeclarationV10 {
    fn from(decl: EnvironmentDeclaration) -> Self {
        Self {
            name: decl.name,
            ty: decl.ty.into(),
            optional: decl.optional,
        }
    }
}

impl From<EnvVarType> for RawEnvVarTypeV10 {
    fn from(ty: EnvVarType) -> Self {
        match ty {
            EnvVarType::String => Self::String,
            EnvVarType::StringLiteral(s) => Self::StringLiteral(s),
            EnvVarType::Union(ss) => Self::Union(ss),
        }
    }
}

impl From<RawEnvironmentDeclarationV10> for EnvironmentDeclaration {
    fn from(decl: RawEnvironmentDeclarationV10) -> Self {
        Self {
            name: decl.name,
            ty: decl.ty.into(),
            optional: decl.optional,
        }
    }
}

impl From<RawEnvVarTypeV10> for EnvVarType {
    fn from(ty: RawEnvVarTypeV10) -> Self {
        match ty {
            RawEnvVarTypeV10::String => Self::String,
            RawEnvVarTypeV10::StringLiteral(s) => Self::StringLiteral(s),
            RawEnvVarTypeV10::Union(ss) => Self::Union(ss),
        }
    }
}

/// An environment schema whose declarations have passed host validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentSchema {
    declarations: std::collections::BTreeMap<String, EnvironmentDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvironmentSchemaError {
    InvalidName,
    TooManyDeclarations,
    TooManyValues,
    ConflictingUpdate { key: String },
    DuplicateDeclaration { key: String },
    EmptyUnion { key: String },
    TooManyUnionEntries { key: String },
    SchemaTooLarge { key: String },
    LiteralTooLarge { key: String },
    MissingRequired { key: String },
    ValueTooLarge { key: String },
    ConstraintMismatch { key: String },
}

impl std::fmt::Display for EnvironmentSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (key, msg) = match self {
            Self::InvalidName => return f.write_str("invalid name"),
            Self::TooManyDeclarations => return f.write_str("too many declarations"),
            Self::TooManyValues => return f.write_str("too many stored values"),
            Self::ConflictingUpdate { key } => (key, "conflicting environment update operations"),
            Self::DuplicateDeclaration { key } => (key, "duplicate declaration"),
            Self::EmptyUnion { key } => (key, "string union must not be empty"),
            Self::TooManyUnionEntries { key } => (key, "string union has too many entries"),
            Self::SchemaTooLarge { key } => (key, "declaration schema exceeds size limit"),
            Self::LiteralTooLarge { key } => (key, "declared literal exceeds value size limit"),
            Self::MissingRequired { key } => (key, "required value is missing"),
            Self::ValueTooLarge { key } => (key, "value exceeds size limit"),
            Self::ConstraintMismatch { key } => (key, "value does not satisfy its declared string constraint"),
        };

        write!(f, "environment key {key:?}: {msg}")
    }
}

impl std::error::Error for EnvironmentSchemaError {}

impl EnvironmentSchema {
    pub const fn empty() -> Self {
        Self {
            declarations: std::collections::BTreeMap::new(),
        }
    }

    fn validate_metadata(declarations: &[EnvironmentDeclaration]) -> Result<(), EnvironmentSchemaError> {
        if declarations.len() > MAX_ENV_VARS {
            return Err(EnvironmentSchemaError::TooManyDeclarations);
        }
        let mut bytes = 0usize;
        for declaration in declarations {
            // Never retain or format unvalidated key bytes in diagnostics.
            validate_key(&declaration.name).map_err(|_| EnvironmentSchemaError::InvalidName)?;
            let key = &declaration.name;
            bytes += declaration.name.len();
            if bytes > MAX_ENV_SCHEMA_BYTES {
                return Err(EnvironmentSchemaError::SchemaTooLarge { key: key.clone() });
            }
            let literals = match &declaration.ty {
                EnvVarType::String => &[][..],
                EnvVarType::StringLiteral(value) => std::slice::from_ref(value),
                EnvVarType::Union(values) => {
                    if values.is_empty() {
                        return Err(EnvironmentSchemaError::EmptyUnion { key: key.clone() });
                    }
                    if values.len() > MAX_ENV_UNION_ENTRIES {
                        return Err(EnvironmentSchemaError::TooManyUnionEntries { key: key.clone() });
                    }
                    values.as_slice()
                }
            };
            for value in literals {
                validate_value(value).map_err(|_| EnvironmentSchemaError::LiteralTooLarge { key: key.clone() })?;
                bytes += value.len();
                if bytes > MAX_ENV_SCHEMA_BYTES {
                    return Err(EnvironmentSchemaError::SchemaTooLarge { key: key.clone() });
                }
            }
        }
        Ok(())
    }

    pub fn new(declarations: Vec<EnvironmentDeclaration>) -> Result<Self, EnvironmentSchemaError> {
        Self::validate_metadata(&declarations)?;
        let mut schema = Self::default();
        for mut declaration in declarations {
            if let EnvVarType::Union(values) = &mut declaration.ty {
                // Literal alternatives form a set, not positional enum variants.
                // Sorting makes declaration order irrelevant and enables binary_search.
                // Equal strings are indistinguishable and deduplicated, so stability is unnecessary.
                values.sort_unstable();
                values.dedup();
            }
            if schema.declarations.contains_key(&declaration.name) {
                return Err(EnvironmentSchemaError::DuplicateDeclaration { key: declaration.name });
            }
            schema.declarations.insert(declaration.name.clone(), declaration);
        }
        Ok(schema)
    }

    /// Check bounds before cloning raw untrusted metadata into the validated schema.
    pub fn from_declarations(declarations: Vec<EnvironmentDeclaration>) -> Result<Self, EnvironmentSchemaError> {
        Self::validate_metadata(&declarations)?;
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

    /// Validate the complete resulting store, including required declarations.
    pub fn validate_values(&self, values: &EnvironmentMap) -> Result<(), EnvironmentSchemaError> {
        self.validate_supplied_values(values)?;
        self.validate_required(values)
    }

    /// Validate supplied values without requiring every required key in this input.
    /// Undeclared values are stored strings but are not readable by module code.
    pub fn validate_supplied_values(&self, values: &EnvironmentMap) -> Result<(), EnvironmentSchemaError> {
        if values.len() > MAX_ENV_VARS {
            return Err(EnvironmentSchemaError::TooManyValues);
        }
        for (name, value) in values {
            validate_key(name).map_err(|_| EnvironmentSchemaError::InvalidName)?;
            validate_value(value).map_err(|_| EnvironmentSchemaError::ValueTooLarge { key: name.clone() })?;
            let Some(declaration) = self.get(name) else { continue };
            let matches = match &declaration.ty {
                EnvVarType::String => true,
                EnvVarType::StringLiteral(expected) => value == expected,
                EnvVarType::Union(allowed) => allowed.binary_search(value).is_ok(),
            };
            if !matches {
                return Err(EnvironmentSchemaError::ConstraintMismatch { key: name.clone() });
            }
        }
        Ok(())
    }

    fn validate_required(&self, values: &EnvironmentMap) -> Result<(), EnvironmentSchemaError> {
        for declaration in self.declarations() {
            if !declaration.optional && !values.contains_key(&declaration.name) {
                return Err(EnvironmentSchemaError::MissingRequired {
                    key: declaration.name.clone(),
                });
            }
        }
        Ok(())
    }
}

/// An environment mutation. Deliberately does not implement Debug because values are secrets.
#[derive(Clone, Default)]
pub struct EnvironmentUpdate {
    pub values: EnvironmentMap,
    pub remove: EnvironmentRemove,
}

#[derive(Clone, Default, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EnvironmentRemove {
    #[default]
    No,
    All,
    Keys(Vec<String>),
}

pub type EnvironmentMap = std::collections::BTreeMap<String, String>;

impl From<EnvironmentMap> for EnvironmentUpdate {
    fn from(values: EnvironmentMap) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }
}

impl EnvironmentUpdate {
    /// Validate operations before any mutation. Diagnostics never include values.
    pub fn validate(&self) -> Result<(), EnvironmentSchemaError> {
        EnvironmentSchema::default().validate_supplied_values(&self.values)?;
        if let EnvironmentRemove::Keys(remove) = &self.remove {
            if remove.len() > MAX_ENV_VARS {
                return Err(EnvironmentSchemaError::TooManyValues);
            }
            let mut seen = std::collections::BTreeSet::new();
            for key in remove {
                validate_key(key).map_err(|_| EnvironmentSchemaError::InvalidName)?;
                if self.values.contains_key(key) || !seen.insert(key) {
                    return Err(EnvironmentSchemaError::ConflictingUpdate { key: key.clone() });
                }
            }
        }
        Ok(())
    }

    /// Resolve the new store without mutating the old store. Schema validation follows
    /// against the deployed schema or the schema from the proposed new module.
    pub fn resulting_values(&self, previous: &EnvironmentMap) -> Result<EnvironmentMap, EnvironmentSchemaError> {
        self.validate()?;
        let mut values = match &self.remove {
            EnvironmentRemove::No => previous.clone(),
            EnvironmentRemove::All => EnvironmentMap::default(),
            EnvironmentRemove::Keys(remove) => {
                let mut values = previous.clone();
                for key in remove {
                    values.remove(key);
                }
                values
            }
        };
        values.extend(self.values.clone());
        EnvironmentSchema::default().validate_supplied_values(&values)?;
        Ok(values)
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    use std::assert_matches;
    use std::collections::BTreeMap;

    fn declaration(name: &str, ty: EnvVarType, optional: bool) -> EnvironmentDeclaration {
        EnvironmentDeclaration {
            name: name.into(),
            ty,
            optional,
        }
    }

    #[test]
    fn complete_inputs_preserve_optional_empty_and_exact_string_constraints() {
        let schema = EnvironmentSchema::new(vec![
            declaration("REQUIRED", EnvVarType::String, false),
            declaration(
                "MODE",
                EnvVarType::Union(vec!["false".into(), "true".into(), "false".into()]),
                false,
            ),
            declaration("OPTIONAL", EnvVarType::StringLiteral("".into()), true),
        ])
        .unwrap();
        let mut values = BTreeMap::from([("REQUIRED".into(), "\0雪".into()), ("MODE".into(), "false".into())]);
        schema.validate_values(&values).unwrap();
        values.insert("OPTIONAL".into(), "".into());
        schema.validate_values(&values).unwrap();
        values.insert("MODE".into(), "False".into());
        assert_matches!(
            schema.validate_values(&values).unwrap_err(),
            EnvironmentSchemaError::ConstraintMismatch { .. }
        );
        values.remove("MODE");
        assert_matches!(
            schema.validate_values(&values).unwrap_err(),
            EnvironmentSchemaError::MissingRequired { .. }
        );
        values.insert("UNDECLARED".into(), "secret-marker".into());
        let error = schema.validate_values(&values).unwrap_err();
        assert_matches!(error, EnvironmentSchemaError::MissingRequired { .. });
        assert!(!format!("{error:?}: {error}").contains("secret-marker"));
    }

    #[test]
    fn publishing_preserves_values_and_validates_the_resulting_declared_subset() {
        let old = BTreeMap::from([("REQUIRED".into(), "ready".into()), ("UNUSED".into(), "secret".into())]);
        let schema = EnvironmentSchema::new(vec![declaration(
            "REQUIRED",
            EnvVarType::StringLiteral("ready".into()),
            false,
        )])
        .unwrap();
        let unchanged = EnvironmentUpdate::default().resulting_values(&old).unwrap();
        assert_eq!(unchanged, old);
        schema.validate_values(&unchanged).unwrap();
        let removed = EnvironmentUpdate {
            remove: EnvironmentRemove::Keys(vec!["REQUIRED".into()]),
            ..Default::default()
        }
        .resulting_values(&old)
        .unwrap();
        assert_matches!(
            schema.validate_values(&removed).unwrap_err(),
            EnvironmentSchemaError::MissingRequired { .. }
        );
        let replace = EnvironmentUpdate {
            remove: EnvironmentRemove::All,
            values: BTreeMap::from([("REQUIRED".into(), "ready".into())]),
        }
        .resulting_values(&old)
        .unwrap();
        schema.validate_values(&replace).unwrap();
        assert!(!replace.contains_key("UNUSED"));
        let newly_declared = EnvironmentSchema::new(vec![declaration(
            "UNUSED",
            EnvVarType::StringLiteral("other".into()),
            false,
        )])
        .unwrap();
        assert_matches!(
            newly_declared.validate_values(&old).unwrap_err(),
            EnvironmentSchemaError::ConstraintMismatch { .. }
        );
        let corrected = EnvironmentUpdate::from(BTreeMap::from([("UNUSED".into(), "other".into())]))
            .resulting_values(&old)
            .unwrap();
        newly_declared.validate_values(&corrected).unwrap();
        assert_eq!(old["UNUSED"], "secret");
    }

    #[test]
    fn mutation_conflicts_and_resulting_store_limit_are_rejected() {
        let stored = (0..MAX_ENV_VARS).map(|i| (format!("KEY{i}"), String::new())).collect();
        assert_matches!(
            EnvironmentUpdate::from(BTreeMap::from([("EXTRA".into(), String::new())]))
                .resulting_values(&stored)
                .unwrap_err(),
            EnvironmentSchemaError::TooManyValues
        );
        for update in [
            EnvironmentUpdate {
                values: BTreeMap::from([("KEY".into(), "secret-marker".into())]),
                remove: EnvironmentRemove::Keys(vec!["KEY".into()]),
            },
            EnvironmentUpdate {
                remove: EnvironmentRemove::Keys(vec!["KEY".into(), "KEY".into()]),
                ..Default::default()
            },
        ] {
            let error = update.validate().unwrap_err();
            assert_matches!(error, EnvironmentSchemaError::ConflictingUpdate { .. });
            assert!(!error.to_string().contains("secret-marker"));
        }
    }

    #[test]
    fn declaration_limits_count_absent_optionals_and_reject_invalid_metadata() {
        let optional = declaration("A", EnvVarType::String, true);
        assert_matches!(
            EnvironmentSchema::new(vec![optional.clone(), optional]).unwrap_err(),
            EnvironmentSchemaError::DuplicateDeclaration { .. }
        );
        assert_matches!(
            EnvironmentSchema::new(
                (0..=MAX_ENV_VARS)
                    .map(|i| declaration(&format!("K{i}"), EnvVarType::String, true))
                    .collect()
            )
            .unwrap_err(),
            EnvironmentSchemaError::TooManyDeclarations
        );
        for (name, constraint, expected) in [
            ("A-B", EnvVarType::String, EnvironmentSchemaError::InvalidName),
            (
                "A",
                EnvVarType::Union(vec![]),
                EnvironmentSchemaError::EmptyUnion { key: "A".into() },
            ),
            (
                "A",
                EnvVarType::StringLiteral("x".repeat(MAX_ENV_VALUE_BYTES + 1)),
                EnvironmentSchemaError::LiteralTooLarge { key: "A".into() },
            ),
        ] {
            assert_eq!(
                EnvironmentSchema::new(vec![declaration(name, constraint, true)]).unwrap_err(),
                expected
            );
        }
        assert!(EnvironmentSchema::default().validate_values(&BTreeMap::new()).is_ok());
        EnvironmentSchema::default()
            .validate_values(&BTreeMap::from([("A".into(), "".into())]))
            .unwrap();
    }

    #[test]
    fn raw_metadata_is_bounded_before_copying_or_formatting_untrusted_keys() {
        let invalid = format!("private-marker\n{}", "x".repeat(100_000));
        let error = EnvironmentSchema::new(vec![declaration(&invalid, EnvVarType::String, true)]).unwrap_err();
        assert!(!format!("{error:?}: {error}").contains("private-marker"));
        let error = EnvironmentSchema::new(vec![declaration(
            "A",
            EnvVarType::Union(vec!["".into(); MAX_ENV_UNION_ENTRIES + 1]),
            true,
        )])
        .unwrap_err();
        assert_matches!(error, EnvironmentSchemaError::TooManyUnionEntries { .. });
        let error = EnvironmentSchema::new(vec![declaration(
            "A",
            EnvVarType::Union(vec!["x".repeat(MAX_ENV_VALUE_BYTES); MAX_ENV_UNION_ENTRIES]),
            true,
        )])
        .unwrap_err();
        assert_matches!(error, EnvironmentSchemaError::SchemaTooLarge { .. });
    }
}
