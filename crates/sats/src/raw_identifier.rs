use crate::algebraic_type::AlgebraicType;
use crate::{impl_deserialize, impl_serialize, impl_st};
use core::borrow::Borrow;
use core::fmt;
use core::ops::Deref;
use lean_string::{LeanString, ToLeanString};

/// A not-yet-validated identifier.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct RawIdentifier(pub(crate) LeanString);

impl_st!([] RawIdentifier, _ts => AlgebraicType::String);
impl_serialize!([] RawIdentifier, (self, ser) => ser.serialize_str(&self.0));
impl_deserialize!([] RawIdentifier, de => LeanString::deserialize(de).map(Self));
impl RawIdentifier {
    /// Creates a new `RawIdentifier` from a string.
    pub fn new(name: impl Into<LeanString>) -> Self {
        Self(name.into())
    }

    pub fn into_inner(self) -> LeanString {
        self.0
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

impl From<&'static str> for RawIdentifier {
    fn from(s: &'static str) -> Self {
        RawIdentifier(LeanString::from_static_str(s))
    }
}

impl From<String> for RawIdentifier {
    fn from(s: String) -> Self {
        RawIdentifier(s.to_lean_string())
    }
}

/// A not-yet-validated, dot-separated name, e.g. `"lib.library_table"`.
///
/// This is the raw counterpart of `spacetimedb_schema`'s `NamespacedIdentifier`,
/// in the same way that [`RawIdentifier`] is the raw counterpart of `Identifier`.
///
/// The distinction matters: a [`RawIdentifier`] can be validated into a single
/// `Identifier`, but a name containing `.` never can be, since `.` is not a legal
/// identifier character. Names that may carry a namespace therefore use this type
/// rather than [`RawIdentifier`], so that the two cannot be confused.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct RawNamespacedIdentifier(LeanString);

impl_st!([] RawNamespacedIdentifier, _ts => AlgebraicType::String);
impl_serialize!([] RawNamespacedIdentifier, (self, ser) => ser.serialize_str(&self.0));
impl_deserialize!([] RawNamespacedIdentifier, de => LeanString::deserialize(de).map(Self));

impl RawNamespacedIdentifier {
    /// Creates a new `RawNamespacedIdentifier` from a string.
    pub fn new(name: impl Into<LeanString>) -> Self {
        Self(name.into())
    }

    /// The dot-separated segments of this name, in order.
    ///
    /// Always yields at least one item; an empty name yields one empty segment.
    pub fn segments(&self) -> impl Iterator<Item = &str> + Clone {
        self.0.split('.')
    }

    /// The final segment, i.e. the name with any namespace prefix stripped.
    ///
    /// `"lib.sessions_id_idx"` yields `"sessions_id_idx"`; an un-namespaced name
    /// yields itself.
    pub fn local_name(&self) -> &str {
        // `rsplit` on a non-empty pattern always yields at least one item.
        self.0.rsplit('.').next().unwrap_or(&self.0)
    }

    /// Whether this name carries a namespace prefix.
    pub fn is_namespaced(&self) -> bool {
        self.0.contains('.')
    }

    pub fn into_inner(self) -> LeanString {
        self.0
    }
}

impl Deref for RawNamespacedIdentifier {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for RawNamespacedIdentifier {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for RawNamespacedIdentifier {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RawNamespacedIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for RawNamespacedIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<RawIdentifier> for RawNamespacedIdentifier {
    /// Every single identifier is a one-segment namespaced name.
    fn from(id: RawIdentifier) -> Self {
        Self(id.0)
    }
}

impl From<&'static str> for RawNamespacedIdentifier {
    fn from(s: &'static str) -> Self {
        Self(LeanString::from_static_str(s))
    }
}

impl From<String> for RawNamespacedIdentifier {
    fn from(s: String) -> Self {
        Self(s.to_lean_string())
    }
}
