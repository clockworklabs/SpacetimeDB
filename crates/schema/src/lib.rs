//! Internal SpacetimeDB schema handling.
//!
//! Handles validation and normalization of raw schema definitions from the `spacetimedb_lib` crate.

pub mod error;
pub mod identifier;
pub mod schema;

/// Helper macro to match against an error stream, expecting a specific error.
/// This lives at the crate root because of how `#[macro_export]` works.
///
/// `$result` is a `Result` whose error holds an error stream,
/// `$expected` is a pattern to match against the error,
/// and `$cond` is an expression that should evaluate to `true` if the error matches.
///
/// Pattern variables from `$expected` can be used in `$cond`.
/// They will be bound behind references.
///
/// Don't use `assert_eq!` in the `$cond`, since multiple matching errors might be present,
/// and the third argument is evaluated for each error.
///
/// ```
/// use spacetimedb_data_structures::ErrorStream;
/// use crate::error::ValidationError;
/// use spacetimedb_sats::AlgebraicTypeRef;
///
/// let result: Result<(), ErrorStream<ValidationError>> =
///     Err(ErrorStream::from(ValidationError::MissingTypeDef { ref_: AlgebraicTypeRef(0) }));
///
/// expect_error_matching!(
///     result,
///     ValidationError::DuplicateColumns { ref_ } => {
///         ref_ == &AlgebraicTypeRef(0)
///     }
/// });
/// ```
#[cfg(test)]
#[macro_export]
macro_rules! expect_error_matching (
    ($result:expr, $expected:pat => $cond:expr) => {
        let result: &::std::result::Result<
            _,
            ::spacetimedb_data_structures::error_stream::ErrorStream<_>
        > = &$result;
        match result {
            Ok(_) => panic!("expected validation error"),
            Err(errors) => {
                let err = errors.iter().find(|error|
                    if let $expected = error {
                        $cond
                    } else {
                        false
                    }
                );
                if let None = err {
                    panic!("expected error matching `{}` satisfying `{}`,\n but got {:#?}", stringify!($expected), stringify!($cond), errors);
                }
            }
        }
    }
);
