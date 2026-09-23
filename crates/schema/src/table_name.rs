use crate::identifier::{Identifier, NamespacedIdentifier};
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, raw_identifier::RawNamespacedIdentifier};

/// The name of a table.
///
/// Root tables have a single-segment name; submodule tables are namespaced
/// (e.g. `"lib.library_table"`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName(NamespacedIdentifier);

impl_st!([] TableName, ts => RawNamespacedIdentifier::make_type(ts));
impl_serialize!([] TableName, (self, ser) => ser.serialize_str(&self.0));
// Stored names were validated when the table was created; segments are trusted
// here, mirroring `Identifier`'s own deserialization.
impl_deserialize!([] TableName, de => RawNamespacedIdentifier::deserialize(de)
    .map(|raw| TableName(NamespacedIdentifier::new_unsafe_assume_valid(&raw))));

impl TableName {
    /// The name of a root table.
    pub fn new(id: Identifier) -> Self {
        Self(id.into())
    }

    /// The final segment, i.e. the name with any namespace prefix stripped.
    pub fn local_name(&self) -> &Identifier {
        self.0.local_name()
    }

    /// Whether this table lives in a submodule namespace.
    pub fn is_namespaced(&self) -> bool {
        self.0.is_namespaced()
    }

    #[cfg(any(test, feature = "test"))]
    pub fn for_test(name: &str) -> Self {
        Self(name.split('.').map(Identifier::for_test).collect())
    }
}

impl From<NamespacedIdentifier> for TableName {
    fn from(id: NamespacedIdentifier) -> Self {
        Self(id)
    }
}

impl From<TableName> for NamespacedIdentifier {
    fn from(id: TableName) -> Self {
        id.0
    }
}

impl Deref for TableName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for TableName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Panics if the `TableName` is a namespaced submodule table name,
/// since those are not single identifiers. Use `NamespacedIdentifier::from`
/// for names that may be namespaced.
impl From<TableName> for Identifier {
    fn from(id: TableName) -> Self {
        id.0.as_identifier()
            .cloned()
            .unwrap_or_else(|| panic!("TableName `{}` is namespaced; use NamespacedIdentifier instead", &*id))
    }
}

// A `TableName` may carry a namespace, so its raw form is a `RawNamespacedIdentifier`,
// not a `RawIdentifier`: the latter can never be validated back into one name.
impl From<TableName> for RawNamespacedIdentifier {
    fn from(id: TableName) -> Self {
        id.0.into()
    }
}

impl fmt::Debug for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}
