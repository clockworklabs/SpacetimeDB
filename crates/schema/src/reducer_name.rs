use crate::identifier::Identifier;
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::raw_identifier::RawIdentifier;

/// The name of a reducer.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReducerName(pub Identifier);

impl ReducerName {
    pub fn new(id: Identifier) -> Self {
        Self(id)
    }

    pub fn for_test(name: &str) -> Self {
        Self(Identifier::for_test(name))
    }

    pub fn as_identifier(&self) -> &Identifier {
        &self.0
    }
}

impl Deref for ReducerName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for ReducerName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ReducerName> for Identifier {
    fn from(id: ReducerName) -> Self {
        id.0
    }
}

impl From<ReducerName> for RawIdentifier {
    fn from(id: ReducerName) -> Self {
        Identifier::from(id).into()
    }
}

impl fmt::Debug for ReducerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for ReducerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}
