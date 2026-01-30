use crate::identifier::Identifier;
use core::ops::Deref;
use core::{borrow::Borrow, fmt};
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::{de::Error, impl_deserialize, impl_serialize, impl_st};

/// The name of a reducer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReducerName(
    // TODO(perf, centril): Use this sort of optimization
    // in RawIdentifier and `Identifier` and more places.
    // TODO(perf): Consider `lean_string` instead for `&'static str` optimization.
    // This could be useful in e.g., `SumType` and friends.
    pub Identifier,
);

impl_st!([] ReducerName, ts => RawIdentifier::make_type(ts));
impl_serialize!([] ReducerName, (self, ser) => self.0.as_raw().serialize(ser));
impl_deserialize!([] ReducerName, de => {
    let raw = RawIdentifier::deserialize(de)?;
    let id = Identifier::new(raw)
        .map_err(|e| Error::custom(format!("invalid identifier: {}", e)))?;
    Ok(ReducerName(id))
});

impl ReducerName {
    pub fn new(id: Identifier) -> Self {
        Self(id)
    }

    #[cfg(feature = "test")]
    pub fn for_test(name: &str) -> Self {
        Self(Identifier::for_test(name))
    }

    pub fn into_identifier(self) -> Identifier {
        self.0
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
