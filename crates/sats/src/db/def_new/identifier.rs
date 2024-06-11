use crate::{db::error::IdentifierError, de::Deserialize, ser::Serialize};
use unicode_ident::{is_xid_continue, is_xid_start};
use unicode_normalization::UnicodeNormalization;

lazy_static::lazy_static! {
    /// TODO(jgilles): Go through these and pull out the partially-reserved ones.
    pub static ref RESERVED_IDENTIFIERS: Vec<&'static str> = include_str!("reserved_identifiers.txt").lines().collect();
}

/// A valid identifier for SpacetimeDB.
/// Identifiers are normalized to Unicode Normalization Form C (canonical decomposition followed by composition), specified by [Unicode Standard Annex 15](https://www.unicode.org/reports/tr15/).
/// Then, they are validated as identifiers according to [Unicode Standard Annex 31](https://www.unicode.org/reports/tr31/). We allow underscores as well as any Unicode XID start character to start an identifier.
/// Some identifiers are reserved and will be rejected. Specifically, any string that uppercases to one of the identifiers in `RESERVED_IDENTIFIERS` will be rejected.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct Identifier {
    // TODO(jgilles): we COULD intern these process-wide...
    name: Box<str>,
}

impl Identifier {
    /// Create a new identifier, or return an error if the name is not a valid identifier.
    pub fn new(name: &str) -> Result<Self, IdentifierError> {
        if name.is_empty() {
            return Err(IdentifierError::Empty {});
        }

        // Convert to Unicode Normalization Form C (canonical decomposition followed by composition).
        let name = name.nfc().to_string();

        let mut chars = name.chars();

        let start = chars.next().expect("non-empty");
        if !is_xid_start(start) && start != '_' {
            return Err(IdentifierError::InvalidStart {
                name: name.clone().into(),
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

        Ok(Self { name: name.into() })
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
}
