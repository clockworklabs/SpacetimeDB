//! A utility for guarding against excessive recursion depth in the SQL parser.
//!
//! Different parts of the parser may have different recursion limits.
//!
//! Removing one could allow the others to be higher, but depending on how the `SQL` is structured, it could lead to a `stack overflow`
//! if is not guarded against, so is incorrect to assume that a limit is sufficient for the next part of the parser.
use crate::parser::errors::{RecursionError, SqlParseError};
use std::fmt::Display;

/// A conservative limit for recursion depth on `parse_expr`.
pub const MAX_RECURSION_EXPR: usize = 700;
/// A conservative limit for recursion depth on `type_expr`.
pub const MAX_RECURSION_TYP_EXPR: usize = 5_000;

/// A utility for guarding against excessive recursion depth.
///
/// **Usage:**
/// ```
/// use spacetimedb_sql_parser::parser::recursion;
/// let mut depth = 0;
/// assert!(recursion::guard(&mut depth, 10, "test").is_ok());
/// ```
pub fn guard(depth: &mut usize, limit: usize, msg: impl Display) -> Result<(), SqlParseError> {
    *depth += 1;
    if *depth > limit {
        Err(RecursionError {
            limit,
            message: msg.to_string(),
        }
        .into())
    } else {
        Ok(())
    }
}
