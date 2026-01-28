use core::ops::Deref;
use core::{borrow::Borrow, fmt};
use ecow::EcoString;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, AlgebraicType};

/// The name of a reducer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ReducerName(
    // TODO(perf, centril): Use this sort of optimization
    // in RawIdentifier and `Identifier` and more places.
    // TODO(perf): Consider `lean_string` instead for `&'static str` optimization.
    // This could be useful in e.g., `SumType` and friends.
    pub EcoString,
);

impl_st!([] ReducerName, _ts => AlgebraicType::String);
impl_serialize!([] ReducerName, (self, ser) => ser.serialize_str(&self.0));
impl_deserialize!([] ReducerName, de => <Box<str>>::deserialize(de).map(|s| Self(EcoString::from(s.as_ref()))));

impl ReducerName {
    pub fn new_from_str(name: &str) -> Self {
        Self(EcoString::from(name))
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

impl Borrow<str> for ReducerName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReducerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}
