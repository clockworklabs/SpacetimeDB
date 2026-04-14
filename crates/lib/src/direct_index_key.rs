#[diagnostic::on_unimplemented(
    message = "column type must be a one of: `u8`, `u16`, `u32`, `u64`, or plain `enum`",
    label = "should be `u8`, `u16`, `u32`, `u64`, or plain `enum`, not `{Self}`"
)]
pub trait DirectIndexKey {}
impl DirectIndexKey for u8 {}
impl DirectIndexKey for u16 {}
impl DirectIndexKey for u32 {}
impl DirectIndexKey for u64 {}

/// Assert that `T` is a valid column to use direct index on.
pub const fn assert_column_type_valid_for_direct_index<T: DirectIndexKey>() {}
