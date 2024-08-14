use spacetimedb_lib::db::raw_def::v9::RawIdentifier;

/// A reason that a string the user used is not allowed.
#[derive(thiserror::Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentifierError {
    /// The identifier is not in Unicode Normalization Form C.
    ///
    /// TODO(1.0): We should *canonicalize* identifiers,
    /// rather than simply *rejecting* non-canonicalized identifiers.
    /// However, this will require careful testing of codegen in both modules and clients,
    /// to ensure that the canonicalization is done consistently.
    /// Otherwise, strange name errors will result.
    #[error(
        "Identifier `{name}` is not in normalization form C according to Unicode Standard Annex 15 \
        (http://www.unicode.org/reports/tr15/) and cannot be used for entities in a module."
    )]
    NotCanonicalized { name: RawIdentifier },

    /// The identifier is reserved.
    #[error("Identifier `{name}` is reserved by spacetimedb and cannot be used for entities in a module.")]
    Reserved { name: RawIdentifier },

    #[error(
        "Identifier `{name}`'s starting character '{invalid_start}' is neither an underscore ('_') nor a \
        Unicode XID_start character (according to Unicode Standard Annex 31, https://www.unicode.org/reports/tr31/) \
        and cannot be used for entities in a module."
    )]
    InvalidStart { name: RawIdentifier, invalid_start: char },

    #[error(
        "Identifier `{name}` contains a character '{invalid_continue}' that is not an XID_continue character \
        (according to Unicode Standard Annex 31, https://www.unicode.org/reports/tr31/) \
        and cannot be used for entities in a module."
    )]
    InvalidContinue {
        name: RawIdentifier,
        invalid_continue: char,
    },

    #[error("Empty identifiers are forbidden.")]
    Empty {},
}
