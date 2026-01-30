use crate::identifier::Identifier;
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, raw_identifier::RawIdentifier};

/// The name of a table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableName(
    // TODO(perf, centril): Use this sort of optimization
    // in RawIdentifier and `Identifier` and more places.
    // TODO(perf): Consider `lean_string` instead for `&'static str` optimization.
    // This could be useful in e.g., `SumType` and friends.
    RawIdentifier,
);

impl_st!([] TableName, ts => RawIdentifier::make_type(ts));
impl_serialize!([] TableName, (self, ser) => self.serialize(ser));
impl_deserialize!([] TableName, de => RawIdentifier::deserialize(de).map(Self));

impl TableName {
    pub fn new(id: Identifier) -> Self {
        Self(id.into_raw())
    }

    #[cfg(feature = "test")]
    pub fn for_test(name: &str) -> Self {
        Self(RawIdentifier::new(name))
    }

    pub fn to_boxed_str(&self) -> Box<str> {
        self.as_ref().into()
    }

    pub fn into_identifier(self) -> Identifier {
        Identifier::new_assume_valid(self.0)
    }

    pub fn into_raw_identifier(self) -> RawIdentifier {
        self.0
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

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}
