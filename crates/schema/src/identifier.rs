use crate::error::IdentifierError;
use spacetimedb_sats::{de, ser};
use std::fmt::{self, Debug, Display};
use std::ops::Deref;
use unicode_ident::{is_xid_continue, is_xid_start};
use unicode_normalization::UnicodeNormalization;

lazy_static::lazy_static! {
    /// TODO(jgilles): Go through these and pull out the partially-reserved ones.
    /// TODO(jgilles): Pull in the rest of the reserved identifiers from Mario's proposal once that's merged.
    pub static ref RESERVED_IDENTIFIERS: Vec<&'static str> = include_str!("reserved_identifiers.txt").lines().collect();
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
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, de::Deserialize, ser::Serialize)]
#[sats(crate = spacetimedb_sats)]
pub struct Identifier {
    // REMARK(jgilles): We *could* intern these.....
    id: Box<str>,
}

impl Identifier {
    /// Validates that the input string is a valid identifier.
    pub fn new(name: &str) -> Result<Self, IdentifierError> {
        if name.is_empty() {
            return Err(IdentifierError::Empty {});
        }

        // Convert to Unicode Normalization Form C (canonical decomposition followed by composition).
        if name.nfc().zip(name.chars()).any(|(a, b)| a != b) {
            // TODO(jgilles): consider whether we should rip this check out.
            // The issue is that the Typespace is currently not canonicalized during validation, so actually canonicalizing identifiers here breaks.
            // Because then you can look up types in the TypeSpace and get back a non-canonicalized identifier, which won't match what you expect...
            // I guess we should just canonicalize the whole Typespace instead.
            // The concern is that generated code will be holding onto non-canonicalized identifiers somewhere which could result in weird, hard-to-find bugs.
            return Err(IdentifierError::NotCanonicalized { name: name.into() });
        }

        let mut chars = name.chars();

        let start = chars.next().expect("non-empty");
        if !is_xid_start(start) && start != '_' {
            return Err(IdentifierError::InvalidStart {
                name: name.into(),
                invalid_start: start,
            });
        }

        for char_ in chars {
            if !is_xid_continue(char_) {
                return Err(IdentifierError::InvalidContinue {
                    name: name.into(),
                    invalid_continue: char_,
                });
            }
        }

        if RESERVED_IDENTIFIERS.contains(&&*name.to_uppercase()) {
            return Err(IdentifierError::Reserved { name: name.into() });
        }

        Ok(Identifier { id: name.into() })
    }
}

impl Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`", self.id)
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl Deref for Identifier {
    type Target = str;

    fn deref(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_bunch_of_identifiers() {
        assert!(Identifier::new("friends").is_ok());
        assert!(Identifier::new("Oysters").is_ok());
        assert!(Identifier::new("_hello").is_ok());
        assert!(Identifier::new("bananas_there_").is_ok());
        assert!(Identifier::new("Москва").is_ok());
        assert!(Identifier::new("東京").is_ok());
        assert!(Identifier::new("bees123").is_ok());

        assert!(Identifier::new("").is_err());
        assert!(Identifier::new("123bees").is_err());
        assert!(Identifier::new("\u{200B}hello").is_err()); // zero-width space
        assert!(Identifier::new(" hello").is_err());
        assert!(Identifier::new("hello ").is_err());
        assert!(Identifier::new("🍌").is_err()); // ;-; the unicode committee is no fun
    }

    #[test]
    fn test_canonicalization() {
        assert!(Identifier::new("_\u{0041}\u{030A}").is_err());
        // canonicalized version of the above.
        assert!(Identifier::new("_\u{00C5}").is_ok());
    }
}
