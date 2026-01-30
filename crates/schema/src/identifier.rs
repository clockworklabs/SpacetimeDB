use crate::error::IdentifierError;
use spacetimedb_data_structures::map::{Equivalent, HashSet};
use spacetimedb_sats::{de, raw_identifier::RawIdentifier, ser};
use std::fmt::{self, Debug, Display};
use std::ops::Deref;
use unicode_ident::{is_xid_continue, is_xid_start};
use unicode_normalization::UnicodeNormalization;

lazy_static::lazy_static! {
    /// TODO(1.0): Pull in the rest of the reserved identifiers from the Identifier Proposal once that's merged.
    static ref RESERVED_IDENTIFIERS: HashSet<&'static str> = include_str!("reserved_identifiers.txt").lines().collect();
}

/// A valid SpacetimeDB Identifier.
///
/// Identifiers must be normalized according to [Unicode Standard Annex 15](https://www.unicode.org/reports/tr15/), normalization form C
/// (Canonical Decomposition followed by Canonical Composition).
/// Following Rust, we use the identifier rules defined by [Unicode Standard Annex 31](https://www.unicode.org/reports/tr31/tr31-37.html) to validate identifiers.
/// We allow underscores as well as any XID_Start character to start an identifier.
///
/// In addition, we forbid the use of any identifier reserved by [PostgreSQL](https://www.postgresql.org/docs/current/sql-keywords-appendix.html).
/// Any string that is converted into a reserved word by the Rust function
/// [`String::to_uppercase`](https://doc.rust-lang.org/std/string/struct.String.html#method.to_uppercase) will be rejected.
///
/// The list of reserved words can be found in the file `SpacetimeDB/crates/sats/db/reserved_identifiers.txt`.
///
/// Internally, this is just a raw identifier with some validation on construction.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, de::Deserialize, ser::Serialize)]
#[sats(crate = spacetimedb_sats)]
pub struct Identifier {
    id: RawIdentifier,
}

impl Identifier {
    /// Returns a new identifier without validating the input.
    pub fn new_assume_valid(name: RawIdentifier) -> Self {
        Self { id: name }
    }

    /// Validates that the input string is a valid identifier.
    ///
    /// Currently, this rejects non-canonicalized identifiers.
    /// Eventually, it will be changed to canonicalize the input string.
    pub fn new(name: RawIdentifier) -> Result<Self, IdentifierError> {
        if name.is_empty() {
            return Err(IdentifierError::Empty {});
        }

        // Convert to Unicode Normalization Form C (canonical decomposition followed by composition).
        if name.nfc().zip(name.chars()).any(|(a, b)| a != b) {
            return Err(IdentifierError::NotCanonicalized { name });
        }

        let mut chars = name.chars();

        let start = chars.next().ok_or(IdentifierError::Empty {})?;
        if !is_xid_start(start) && start != '_' {
            return Err(IdentifierError::InvalidStart {
                name,
                invalid_start: start,
            });
        }

        for char_ in chars {
            if !is_xid_continue(char_) {
                return Err(IdentifierError::InvalidContinue {
                    name,
                    invalid_continue: char_,
                });
            }
        }

        if Identifier::is_reserved(&name) {
            return Err(IdentifierError::Reserved { name });
        }

        Ok(Identifier { id: name })
    }

    #[cfg(any(test, feature = "test"))]
    pub fn for_test(name: impl AsRef<str>) -> Self {
        Identifier::new(RawIdentifier::new(name.as_ref())).unwrap()
    }

    /// Returns the raw identifier of this identifier.
    pub fn into_raw(self) -> RawIdentifier {
        self.id
    }

    /// Returns the raw identifier of this identifier.
    pub fn as_raw(&self) -> &RawIdentifier {
        &self.id
    }

    /// Check if a string is a reserved identifier.
    pub fn is_reserved(name: &str) -> bool {
        RESERVED_IDENTIFIERS.contains(&*name.to_uppercase())
    }
}

impl Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.id, f)
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.id, f)
    }
}

impl Deref for Identifier {
    type Target = str;

    fn deref(&self) -> &str {
        &self.id
    }
}

impl Equivalent<Identifier> for str {
    fn equivalent(&self, other: &Identifier) -> bool {
        self == &other.id[..]
    }
}

impl From<Identifier> for RawIdentifier {
    fn from(id: Identifier) -> Self {
        id.into_raw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn new(s: &str) -> Result<Identifier, IdentifierError> {
        Identifier::new(RawIdentifier::new(s))
    }

    #[test]
    fn test_a_bunch_of_identifiers() {
        assert!(new("friends").is_ok());
        assert!(new("Oysters").is_ok());
        assert!(new("_hello").is_ok());
        assert!(new("bananas_there_").is_ok());
        assert!(new("Москва").is_ok());
        assert!(new("東京").is_ok());
        assert!(new("bees123").is_ok());

        assert!(new("").is_err());
        assert!(new("123bees").is_err());
        assert!(new("\u{200B}hello").is_err()); // zero-width space
        assert!(new(" hello").is_err());
        assert!(new("hello ").is_err());
        assert!(new("🍌").is_err()); // ;-; the unicode committee is no fun
        assert!(new("").is_err());
    }

    #[test]
    fn test_canonicalization() {
        assert!(new("_\u{0041}\u{030A}").is_err());
        // canonicalized version of the above.
        assert!(new("_\u{00C5}").is_ok());
    }

    proptest! {
        #[test]
        fn test_standard_ascii_identifiers(s in "[a-zA-Z_][a-zA-Z0-9_]*") {
            // Ha! Proptest will reliably find these.
            prop_assume!(!Identifier::is_reserved(&s));

            prop_assert!(new(&s).is_ok());
        }
    }
}
