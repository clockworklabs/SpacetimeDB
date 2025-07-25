//! A utility for guarding against stack overflows in the SQL parser.
//!
//! Different parts of the parser may have different recursion limits, based in the size of the structures they parse.

use crate::parser::errors::{RecursionError, SqlParseError};

/// A conservative limit for recursion depth on `parse_expr`.
pub const MAX_RECURSION_EXPR: usize = 1_600;
/// A conservative limit for recursion depth on `type_expr`.
pub const MAX_RECURSION_TYP_EXPR: usize = 2_500;

/// A utility for guarding against stack overflows in the SQL parser.
///
/// **Usage:**
/// ```
/// use spacetimedb_sql_parser::parser::recursion;
/// let mut depth = 0;
/// assert!(recursion::guard(depth, 10, "test").is_ok());
/// ```
pub fn guard(depth: usize, limit: usize, source: &'static str) -> Result<(), SqlParseError> {
    if depth > limit {
        Err(RecursionError { source_: source }.into())
    } else {
        Ok(())
    }
}
