use crate::identifier::Identifier;
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, raw_identifier::RawIdentifier};

/// The name of a table.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TableName(Identifier);

impl_st!([] TableName, ts => Identifier::make_type(ts));
impl_serialize!([] TableName, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] TableName, de => Identifier::deserialize(de).map(Self));

impl TableName {
    pub fn new(id: Identifier) -> Self {
        Self(id)
    }

    #[cfg(feature = "test")]
    pub fn for_test(name: &str) -> Self {
        Self(Identifier::for_test(name))
    }

    pub fn to_boxed_str(&self) -> Box<str> {
        self.as_ref().into()
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
        id.0
    }
}

impl From<TableName> for RawIdentifier {
    fn from(id: TableName) -> Self {
        Identifier::from(id).into()
    }
}

impl fmt::Debug for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}
