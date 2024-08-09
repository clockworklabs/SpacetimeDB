use crate::error::ValidationErrors;

/// Validation code for various versions of ModuleDef.
pub mod v8;
pub mod v9;

pub type Result<T> = std::result::Result<T, ValidationErrors>;

/// Helpers used in tests for validation modules.
#[cfg(test)]
mod tests {
    use spacetimedb_primitives::ColList;

    use crate::identifier::Identifier;

    /// Create a column list, panicking if the data is invalid.
    pub fn expect_col_list<const N: usize>(data: [usize; N]) -> ColList {
        ColList::try_from_iter(data).expect("invalid column list")
    }

    /// Create an identifier, panicking if invalid.
    pub fn expect_identifier(data: impl Into<Box<str>>) -> Identifier {
        Identifier::new(data.into()).expect("invalid identifier")
    }
}
