use crate::algebraic_type::AlgebraicType;
use crate::{impl_deserialize, impl_serialize, impl_st};
use core::borrow::Borrow;
use core::fmt;
use core::ops::Deref;
use ecow::EcoString;

/// A not-yet-validated identifier.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
// TODO(perf): Consider `lean_string` instead for `&'static str` optimization.
// This could be useful in e.g., `SumType` and friends.
pub struct RawIdentifier(pub(crate) EcoString);

impl_st!([] RawIdentifier, _ts => AlgebraicType::String);
impl_serialize!([] RawIdentifier, (self, ser) => ser.serialize_str(&self.0));
impl_deserialize!([] RawIdentifier, de => EcoString::deserialize(de).map(Self));

impl RawIdentifier {
    /// Creates a new `RawIdentifier` from a string.
    pub fn new(name: impl Into<EcoString>) -> Self {
        Self(name.into())
    }
}

impl Deref for RawIdentifier {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for RawIdentifier {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for RawIdentifier {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RawIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for RawIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

// This impl exists to facilitate optimizations in the future.
impl From<&'static str> for RawIdentifier {
    fn from(s: &'static str) -> Self {
        RawIdentifier::new(s)
    }
}
