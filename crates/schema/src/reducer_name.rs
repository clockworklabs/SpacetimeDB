use crate::identifier::{Identifier, NamespacedIdentifier};
use core::fmt;
use core::ops::Deref;
use spacetimedb_sats::raw_identifier::{RawIdentifier, RawNamespacedIdentifier};

/// The name of a reducer.
///
/// Fully qualified: a reducer in a submodule mounted as `myauth` is named
/// `myauth.verify_token`, which is how clients address it. Use [`ReducerName::local`]
/// for the name within its own module.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReducerName(pub NamespacedIdentifier);

impl ReducerName {
    pub fn new(id: impl Into<NamespacedIdentifier>) -> Self {
        Self(id.into())
    }

    pub fn for_test(name: &str) -> Self {
        Self(name.split('.').map(Identifier::for_test).collect())
    }

    /// The reducer's name within its own module, without the namespace it is mounted under.
    pub fn local(&self) -> &Identifier {
        self.0.local_name()
    }

    pub fn as_namespaced(&self) -> &NamespacedIdentifier {
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

impl From<ReducerName> for NamespacedIdentifier {
    fn from(id: ReducerName) -> Self {
        id.0
    }
}

impl From<ReducerName> for RawNamespacedIdentifier {
    fn from(id: ReducerName) -> Self {
        id.0.into()
    }
}

impl From<ReducerName> for RawIdentifier {
    /// Flattens the qualified name; only for the wire types that carry it as a plain string.
    fn from(id: ReducerName) -> Self {
        RawIdentifier::new(RawNamespacedIdentifier::from(id.0).into_inner())
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
