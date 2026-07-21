use crate::identifier::{Identifier, NamespacedIdentifier};
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, raw_identifier::RawIdentifier};

/// The name of a table.
///
/// Root tables have a single-segment name; submodule tables are namespaced
/// (e.g. `"lib.library_table"`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName(NamespacedIdentifier);

impl_st!([] TableName, ts => RawIdentifier::make_type(ts));
impl_serialize!([] TableName, (self, ser) => ser.serialize_str(&self.0));
// Stored names were validated when created; segments are trusted here,
// mirroring `Identifier`'s own deserialization.
impl_deserialize!([] TableName, de => RawIdentifier::deserialize(de).map(|raw| {
    TableName(
        raw.split('.')
            .map(|segment| Identifier::new_assume_valid(RawIdentifier::new(segment)))
            .collect(),
    )
}));

impl TableName {
    /// The name of a root table.
    pub fn new(id: Identifier) -> Self {
        Self(id.into())
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
        match id.0.segments() {
            [single] => single.clone(),
            _ => panic!("TableName `{}` is namespaced; use NamespacedIdentifier instead", &*id),
        }
    }
}

impl From<TableName> for RawIdentifier {
    fn from(id: TableName) -> Self {
        RawIdentifier::new(&*id.0)
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
