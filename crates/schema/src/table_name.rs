use crate::identifier::Identifier;
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, raw_identifier::RawIdentifier};

/// The name of a table.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName(RawIdentifier);

impl_st!([] TableName, ts => RawIdentifier::make_type(ts));
impl_serialize!([] TableName, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] TableName, de => RawIdentifier::deserialize(de).map(Self));

impl TableName {
    /// Construct from a validated identifier (all user-defined tables).
    pub fn new(id: Identifier) -> Self {
        Self(id.into())
    }

    /// Construct from an arbitrary raw string (e.g. mounted tables whose names contain `.`).
    pub fn new_raw(name: RawIdentifier) -> Self {
        Self(name)
    }

    #[cfg(any(test, feature = "test"))]
    pub fn for_test(name: &str) -> Self {
        Self(RawIdentifier::from(name))
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

impl From<TableName> for Identifier {
    fn from(id: TableName) -> Self {
        Identifier::new(id.0).expect("TableName contains '.' or other non-identifier chars; use RawIdentifier instead")
    }
}

impl From<TableName> for RawIdentifier {
    fn from(id: TableName) -> Self {
        id.0
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
