//! Provides a trait [`ReadColumn`] for extracting a single column from a [`crate::table::RowRef`].
//! This is desirable as frequently, e.g. when evaluating filtered queries,
//! we are interested in only a single column (or a small set of columns),
//! and would like to avoid the allocation required by a `ProductValue`.

use crate::{bflatn_from, indexes::PageOffset, table::RowRef};
use spacetimedb_sats::layout::{AlgebraicTypeLayout, PrimitiveType, ProductTypeElementLayout, Size, VarLenType};
use spacetimedb_sats::{
    algebraic_value::{ser::ValueSerializer, Packed},
    i256,
    sum_value::SumTag,
    u256, AlgebraicType, AlgebraicValue, ArrayValue, ProductType, ProductValue, SumValue, F32, F64,
};
use std::{cell::Cell, mem};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TypeError {
    #[error(
        "Attempt to read column {} of a product with only {} columns of type {:?}",
        desired,
        found.elements.len(),
        found,
    )]
    IndexOutOfBounds { desired: usize, found: ProductType },
    #[error("Attempt to read a column at type `{desired}`, but the column's type is {found:?}")]
    WrongType {
        desired: &'static str,
        found: AlgebraicType,
    },
}

/// Types which can be stored in a column of a row,
/// and can be extracted directly from a row.
///
/// # Safety
///
/// The implementor must define `is_compatible_type` to return `true` only for `AlgebraicTypeLayout`s
/// for which `unchecked_read_column` is safe.
/// The provided `read_column` method uses `is_compatible_type` to detect type errors,
/// and calls `unchecked_read_column` if `is_compatible_type` returns true.
pub unsafe trait ReadColumn: Sized {
    /// Is `ty` compatible with `Self`?
    ///
    /// The definition of "compatible" here is left to the implementor,
    /// to be defined by `Self::is_compatible_type`.
    ///
    /// For most types,"compatibility" will mean that each Rust type which implements `ReadColumn`
    /// has exactly one corresponding [`AlgebraicTypeLayout`] which represents it,
    /// and the column in `table.row_layout` must be of that type.
    ///
    /// Notable exceptions are [`AlgebraicValue`], [`ProductValue`] and [`SumValue`].
    /// Any `ProductTypeLayout` is compatible with `ProductValue`,
    /// any `SumTypeLayout` is compatible with `SumValue`,
    /// and any `AlgebraicTypeLayout` at all is compatible with `AlgebraicValue`.
    fn is_compatible_type(ty: &AlgebraicTypeLayout) -> bool;

    /// Extract a value of type `Self` from the row pointed to by `row_ref`
    /// which is stored in the column defined by `layout`.
    ///
    /// # Safety
    ///
    /// `layout` must appear as a column in the `table.row_layout.product().elements`,
    /// *not* to a nested field of a column which is a product or sum value.
    /// That column must have the same layout as `layout`.
    /// This restriction may be loosened in the future.
    ///
    /// Assuming that the `row_ref` refers to a properly-aligned row,
    /// adding the `layout.offset` must result in a properly-aligned value of that compatible type.
    ///
    /// `layout.ty` must be compatible with `Self`.
    /// The definition of "compatible" here is left to the implementor,
    /// to be defined by `Self::is_compatible_type`.
    ///
    /// For most types,"compatibility" will mean that each Rust type which implements `ReadColumn`
    /// has exactly one corresponding [`AlgebraicTypeLayout`] which represents it,
    /// and the column in `table.row_layout` must be of that type.
    ///
    /// Notable exceptions are [`AlgebraicValue`], [`ProductValue`] and [`SumValue`].
    /// Any `ProductTypeLayout` is compatible with `ProductValue`,
    /// any `SumTypeLayout` is compatible with `SumValue`,
    /// and any `AlgebraicTypeLayout` at all is compatible with `AlgebraicValue`.
    ///
    /// # Notes for implementors
    ///
    /// Implementors may depend on all of the above safety requirements,
    /// and on the validity of the `row_ref`.
    /// Assuming all of the above safety requirements are met and the `row_ref` refers to a valid row,
    /// this method *must never* invoke Undefined Behavior.
    ///
    /// Implementors should carefully study the BFLATN format.
    /// Currently BFLATN lacks a normative specification,
    /// so implementors should read the definitions in [`layout.rs`], [`bflatn_to.rs`] and [`bflatn_from.rs`].
    /// A few highlights are included here:
    ///
    /// - Variable-length columns, i.e. `AlgebraicType::String`, `AlgebraicType::Array` and `AlgebraicType::Map`
    ///   are stored within the row as [`crate::var_len::VarLenRef`s],
    ///   which refer to an intrusive linked list of 62-byte "granules",
    ///   allocated separately in a space starting from the end of the page.
    ///   Strings are stored as UTF-8 bytes; all other var-len types are stored as BSATN-encoded bytes.
    ///
    /// - Fixed-length columns, i.e. all types not listed above as variable-length,
    ///   are stored inline at a known offset.
    ///   Their layout generally matches the C ABI on an x86_64 Linux machine,
    ///   with the notable exception of sum types, since the C ABI doesn't define a layout for sums.
    ///
    /// - Fixed-length columns are stored in order, with padding between to ensure proper alignment.
    ///
    /// - Primitive (non-compound) fixed-length types, i.e. integers, floats and booleans,
    ///   have alignment equal to their size.
    ///
    /// - Integers are stored little-endian.
    ///
    /// - Floats are stored by bitwise converting to integers as per IEEE-754,
    ///   then storing those integers little-endian.
    ///
    /// - Booleans are stored as `u8`, i.e. bytes, restricted to the values `0` and `1`.
    ///
    /// - Products store their elements in order, with padding between to ensure proper alignment.
    ///
    /// - The first element of a product has offset 0.
    ///
    /// - The alignment of a product is the maximum alignment of its elements,
    ///   or 1 for the empty product.
    ///
    /// - The size of a product is the number of bytes required to store its elements, including padding,
    ///   plus trailing padding bytes so that the size is a multiple of the alignment.
    ///
    /// - Sums store their payload at offset 0, followed by a 1-byte tag.
    ///
    /// - The alignment of a sum is the maximum alignment of its variants' payloads.
    ///
    /// - The size of a sum is the maximum size of its variants' payloads, plus 1 (the tag),
    ///   plus trailing padding bytes so that the size is a multiple of the alignment.
    ///
    /// - The offset of a sum's tag bit is the maximum size of its variants' payloads.
    unsafe fn unchecked_read_column(row_ref: RowRef<'_>, layout: &ProductTypeElementLayout) -> Self;

    /// Check that the `idx`th column of the row type stored by `row_ref` is compatible with `Self`,
    /// and read the value of that column from `row_ref`.
    fn read_column(row_ref: RowRef<'_>, idx: usize) -> Result<Self, TypeError> {
        let layout = row_ref.row_layout().product();

        // Look up the `ProductTypeElementLayout` of the requested column,
        // or return an error on an out-of-bounds index.
        let col = layout.elements.get(idx).ok_or_else(|| TypeError::IndexOutOfBounds {
            desired: idx,
            found: layout.product_type(),
        })?;

        // Check that the requested column is of the expected type.
        if !Self::is_compatible_type(&col.ty) {
            return Err(TypeError::WrongType {
                desired: std::any::type_name::<Self>(),
                found: col.ty.algebraic_type(),
            });
        }

        Ok(unsafe {
            // SAFETY:
            // - We trust that the `row_ref.table` knows its own layout,
            //   and we've derived our type and layout info from it,
            //   so they are correct.
            // - We trust `Self::is_compatible_type`, and it returned `true`,
            //   so the column must be of appropriate type.
            Self::unchecked_read_column(row_ref, col)
        })
    }
}

unsafe impl ReadColumn for bool {
    fn is_compatible_type(ty: &AlgebraicTypeLayout) -> bool {
        matches!(ty, AlgebraicTypeLayout::Primitive(PrimitiveType::Bool))
    }

    unsafe fn unchecked_read_column(row_ref: RowRef<'_>, layout: &ProductTypeElementLayout) -> Self {
        debug_assert!(Self::is_compatible_type(&layout.ty));

        let (page, offset) = row_ref.page_and_offset();
        let col_offset = offset + PageOffset(layout.offset);

        let data = page.get_row_data(col_offset, Size(mem::size_of::<Self>() as u16));
        let data: *const bool = data.as_ptr().cast();
        // SAFETY: We trust that the `row_ref` refers to a valid, initialized row,
        // and that the `offset_in_bytes` refers to a column of type `Bool` within that row.
        // A valid row can never have a column of an invalid value,
        // and no byte in `Page.row_data` is ever uninit,
        // so `data` must be initialized as either 0 or 1.
        unsafe { *data }
    }
}

macro_rules! impl_read_column_number {
    ($primitive_type:ident => $native_type:ty) => {
        unsafe impl ReadColumn for $native_type {
            fn is_compatible_type(ty: &AlgebraicTypeLayout) -> bool {
                matches!(ty, AlgebraicTypeLayout::Primitive(PrimitiveType::$primitive_type))
            }

            unsafe fn unchecked_read_column(
                row_ref: RowRef<'_>,
                layout: &ProductTypeElementLayout,
            ) -> Self {
                debug_assert!(Self::is_compatible_type(&layout.ty));

                let (page, offset) = row_ref.page_and_offset();
                let col_offset = offset + PageOffset(layout.offset);

                let data = page.get_row_data(col_offset, Size(mem::size_of::<Self>() as u16));
                let data: Result<[u8; mem::size_of::<Self>()], _> = data.try_into();
                // SAFETY: `<[u8; N] as TryFrom<&[u8]>` succeeds if and only if the slice's length is `N`.
                // We used `mem::size_of::<Self>()` as both the length of the slice and the array,
                // so we know them to be equal.
                let data = unsafe { data.unwrap_unchecked() };

                Self::from_le_bytes(data)
            }
        }
    };

    ($($primitive_type:ident => $native_type:ty);* $(;)*) => {
        $(impl_read_column_number!($primitive_type => $native_type);)*
    };
}

impl_read_column_number! {
    I8 => i8;
    U8 => u8;
    I16 => i16;
    U16 => u16;
    I32 => i32;
    U32 => u32;
    I64 => i64;
    U64 => u64;
    I128 => i128;
    U128 => u128;
    I256 => i256;
    U256 => u256;
    F32 => f32;
    F64 => f64;
}

unsafe impl ReadColumn for AlgebraicValue {
    fn is_compatible_type(_ty: &AlgebraicTypeLayout) -> bool {
        true
    }
    unsafe fn unchecked_read_column(row_ref: RowRef<'_>, layout: &ProductTypeElementLayout) -> Self {
        let curr_offset = Cell::new(layout.offset as usize);
        let blob_store = row_ref.blob_store();
        let (page, page_offset) = row_ref.page_and_offset();
        let fixed_bytes = page.get_row_data(page_offset, row_ref.row_layout().size());

        // SAFETY:
        // 1. Our requirements on `row_ref` and `layout` mean that the column is valid at `layout`.
        // 2. As a result of the above, all `VarLenRef`s in the column are valid.
        // 3. Our requirements on `offset_in_bytes` mean that our `curr_offset` is valid.
        let res = unsafe {
            bflatn_from::serialize_value(ValueSerializer, fixed_bytes, page, blob_store, &curr_offset, &layout.ty)
        };

        debug_assert!(res.is_ok());

        // SAFETY: `ValueSerializer` is infallible.
        unsafe { res.unwrap_unchecked() }
    }
}

macro_rules! impl_read_column_via_av {
    ($av_pattern:pat => $into_method:ident => $native_type:ty) => {
        unsafe impl ReadColumn for $native_type {
            fn is_compatible_type(ty: &AlgebraicTypeLayout) -> bool {
                matches!(ty, $av_pattern)
            }

            unsafe fn unchecked_read_column(
                row_ref: RowRef<'_>,
                layout: &ProductTypeElementLayout,
            ) -> Self {
                debug_assert!(Self::is_compatible_type(&layout.ty));

                // SAFETY:
                // - Any layout is valid for `AlgebraicValue`, including our `layout`.
                // - Forward requirements on `offset_in_bytes`.
                let av = unsafe { AlgebraicValue::unchecked_read_column(row_ref, layout) };

                let res = av.$into_method();

                debug_assert!(res.is_ok());

                // SAFETY: We trust that the value `row_ref + offset_in_bytes` is of type `layout`,
                // and that `layout` is the layout for `Self`,
                // so the `av` above must be a `Self`.
                unsafe { res.unwrap_unchecked() }
            }
        }
    };

    ($($av_pattern:pat => $into_method:ident => $native_type:ty);* $(;)*) => {
        $(impl_read_column_via_av!($av_pattern => $into_method => $native_type);)*
    };
}

impl_read_column_via_av! {
    AlgebraicTypeLayout::VarLen(VarLenType::String) => into_string => Box<str>;
    AlgebraicTypeLayout::VarLen(VarLenType::Array(_)) => into_array => ArrayValue;
    AlgebraicTypeLayout::Sum(_) => into_sum => SumValue;
    AlgebraicTypeLayout::Product(_) => into_product => ProductValue;
}

macro_rules! impl_read_column_via_from {
    ($($base:ty => $target:ty);* $(;)*) => {
        $(
            unsafe impl ReadColumn for $target {
                fn is_compatible_type(ty: &AlgebraicTypeLayout) -> bool {
                    <$base>::is_compatible_type(ty)
                }

                unsafe fn unchecked_read_column(row_ref: RowRef<'_>, layout: &ProductTypeElementLayout) -> Self {
                    // SAFETY: We use `$base`'s notion of compatible types, so we can forward promises.
                    <$target>::from(unsafe { <$base>::unchecked_read_column(row_ref, layout) })
                }
            }
        )*
    };
}

impl_read_column_via_from! {
    u16 => spacetimedb_primitives::ColId;
    u32 => spacetimedb_primitives::ViewId;
    u32 => spacetimedb_primitives::TableId;
    u32 => spacetimedb_primitives::IndexId;
    u32 => spacetimedb_primitives::ConstraintId;
    u32 => spacetimedb_primitives::SequenceId;
    u32 => spacetimedb_primitives::ScheduleId;
    u128 => Packed<u128>;
    i128 => Packed<i128>;
    u256 => Box<u256>;
    i256 => Box<i256>;
    f32 => F32;
    f64 => F64;
}

/// SAFETY: `is_compatible_type` only returns true for sum types,
/// and any sum value stores the tag first in BFLATN.
unsafe impl ReadColumn for SumTag {
    fn is_compatible_type(ty: &AlgebraicTypeLayout) -> bool {
        matches!(ty, AlgebraicTypeLayout::Sum(_))
    }

    unsafe fn unchecked_read_column(row_ref: RowRef<'_>, layout: &ProductTypeElementLayout) -> Self {
        debug_assert!(Self::is_compatible_type(&layout.ty));

        let (page, offset) = row_ref.page_and_offset();
        let col_offset = offset + PageOffset(layout.offset);

        let data = page.get_row_data(col_offset, Size(1));
        let data: Result<[u8; 1], _> = data.try_into();
        // SAFETY: `<[u8; 1] as TryFrom<&[u8]>` succeeds if and only if the slice's length is `1`.
        // We used `1` as both the length of the slice and the array, so we know them to be equal.
        let [data] = unsafe { data.unwrap_unchecked() };

        Self(data)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::table::test::table;
    use crate::{blob_store::HashMapBlobStore, page_pool::PagePool};
    use proptest::{prelude::*, prop_assert_eq, proptest, test_runner::TestCaseResult};
    use spacetimedb_sats::{product, proptest::generate_typed_row};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(if cfg!(miri) { 8 } else { 2048 }))]

        #[test]
        /// Test that `AlgebraicValue::read_column` returns expected values.
        ///
        /// That is, test that, for any row type and any row value,
        /// inserting the row, then doing `AlgebraicValue::read_column` on each column of the row
        /// returns the expected value.
        fn read_column_same_value((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);

            let (_, row_ref) = table.insert(&pool, &mut blob_store, &val).unwrap();

            for (idx, orig_col_value) in val.into_iter().enumerate() {
                let read_col_value = row_ref.read_col::<AlgebraicValue>(idx).unwrap();
                prop_assert_eq!(orig_col_value, read_col_value);
            }
        }

        #[test]
        /// Test that trying to read a column at a type more specific than `AlgebraicValue`
        /// which does not match the actual column type
        /// returns an appropriate error.
        fn read_column_wrong_type((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty.clone());

            let (_, row_ref) = table.insert(&pool, &mut blob_store, &val).unwrap();

            for (idx, col_ty) in ty.elements.iter().enumerate() {
                assert_wrong_type_error::<u8>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::U8)?;
                assert_wrong_type_error::<i8>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::I8)?;
                assert_wrong_type_error::<u16>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::U16)?;
                assert_wrong_type_error::<i16>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::I16)?;
                assert_wrong_type_error::<u32>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::U32)?;
                assert_wrong_type_error::<i32>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::I32)?;
                assert_wrong_type_error::<u64>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::U64)?;
                assert_wrong_type_error::<i64>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::I64)?;
                assert_wrong_type_error::<u128>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::U128)?;
                assert_wrong_type_error::<i128>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::I128)?;
                assert_wrong_type_error::<u256>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::U256)?;
                assert_wrong_type_error::<i256>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::I256)?;
                assert_wrong_type_error::<f32>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::F32)?;
                assert_wrong_type_error::<f64>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::F64)?;
                assert_wrong_type_error::<bool>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::Bool)?;
                assert_wrong_type_error::<Box<str>>(row_ref, idx, &col_ty.algebraic_type, AlgebraicType::String)?;
            }
        }

        #[test]
        /// Test that trying to read a column which does not exist,
        /// i.e. with an out-of-bounds index,
        /// returns an appropriate error.
        fn read_column_out_of_bounds((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty.clone());

            let (_, row_ref) = table.insert(&pool, &mut blob_store, &val).unwrap();

            let oob = ty.elements.len();

            match row_ref.read_col::<AlgebraicValue>(oob) {
                Err(TypeError::IndexOutOfBounds { desired, found }) => {
                    prop_assert_eq!(desired, oob);
                    // Constructing a table changes the `ProductType` by adding column names
                    // if the type has `None` for its element names,
                    // so we can't blindly `prop_assert_eq!(found, ty)`.
                    // Instead, check that they have the same number of elements
                    // and that each element has the same type.
                    prop_assert_eq!(found.elements.len(), ty.elements.len());
                    for (found_col, ty_col) in found.elements.iter().zip(ty.elements.iter()) {
                        prop_assert_eq!(&found_col.algebraic_type, &ty_col.algebraic_type);
                    }
                }
                Err(e) => panic!("Expected TypeError::IndexOutOfBounds but found {e:?}"),
                Ok(val) => panic!("Expected error but found Ok({val:?})"),
            }
        }
    }

    /// Assert, if and only if `col_ty` is not `correct_col_ty`,
    /// that `row_ref.read_col::<Col>(col_idx)` returns a `TypeError::WrongType`.
    ///
    /// If `col_ty == correct_col_ty`, do nothing.
    fn assert_wrong_type_error<Col: ReadColumn + PartialEq + std::fmt::Debug>(
        row_ref: RowRef<'_>,
        col_idx: usize,
        col_ty: &AlgebraicType,
        correct_col_ty: AlgebraicType,
    ) -> TestCaseResult {
        if col_ty != &correct_col_ty {
            match row_ref.read_col::<Col>(col_idx) {
                Err(TypeError::WrongType { desired, found }) => {
                    prop_assert_eq!(desired, std::any::type_name::<Col>());
                    prop_assert_eq!(&found, col_ty);
                }
                Err(e) => panic!("Expected TypeError::WrongType but found {e:?}"),
                Ok(val) => panic!("Expected error but found Ok({val:?})"),
            }
        }
        Ok(())
    }

    /// Define a test or tests which constructs a row containing a known value of a known type,
    /// then uses `ReadColumn::read_column` to extract that type as a native type,
    /// e.g. a Rust integer,
    /// and asserts that the extracted value is as expected.
    macro_rules! test_read_column_primitive {
        ($name:ident { $algebraic_type:expr => $rust_type:ty = $val:expr }) => {
            #[test]
            fn $name() {
                let pool = PagePool::new_for_test();
                let mut blob_store = HashMapBlobStore::default();
                let mut table = table(ProductType::from_iter([$algebraic_type]));

                let val: $rust_type = $val;
                let (_, row_ref) = table.insert(&pool, &mut blob_store, &product![val.clone()]).unwrap();

                assert_eq!(val, row_ref.read_col::<$rust_type>(0).unwrap());
            }
        };


        ($($name:ident { $algebraic_type:expr => $rust_type:ty = $val:expr };)*) => {
            $(test_read_column_primitive! {
                $name { $algebraic_type => $rust_type = $val }
            })*
        }
    }

    test_read_column_primitive! {
        read_column_i8 { AlgebraicType::I8 => i8 = i8::MAX };
        read_column_u8 { AlgebraicType::U8 => u8 = 0xa5 };
        read_column_i16 { AlgebraicType::I16 => i16 = i16::MAX };
        read_column_u16 { AlgebraicType::U16 => u16 = 0xa5a5 };
        read_column_i32 { AlgebraicType::I32 => i32 = i32::MAX };
        read_column_u32 { AlgebraicType::U32 => u32 = 0xa5a5a5a5 };
        read_column_i64 { AlgebraicType::I64 => i64 = i64::MAX };
        read_column_u64 { AlgebraicType::U64 => u64 = 0xa5a5a5a5_a5a5a5a5 };
        read_column_i128 { AlgebraicType::I128 => i128 = i128::MAX };
        read_column_u128 { AlgebraicType::U128 => u128 = 0xa5a5a5a5_a5a5a5a5_a5a5a5a5_a5a5a5a5 };
        read_column_i256 { AlgebraicType::I256 => i256 = i256::MAX };
        read_column_u256 { AlgebraicType::U256 => u256 =
            u256::from_words(
                0xa5a5a5a5_a5a5a5a5_a5a5a5a5_a5a5a5a5,
                0xa5a5a5a5_a5a5a5a5_a5a5a5a5_a5a5a5a5
            )
        };

        read_column_f32 { AlgebraicType::F32 => f32 = 1.0 };
        read_column_f64 { AlgebraicType::F64 => f64 = 1.0 };

        read_column_bool { AlgebraicType::Bool => bool = true };

        read_column_empty_string { AlgebraicType::String => Box<str> = "".into() };

        // Use a short string which fits in a single granule.
        read_column_short_string { AlgebraicType::String => Box<str> = "short string".into() };

        // Use a medium-sized string which takes multiple granules.
        read_column_medium_string { AlgebraicType::String => Box<str> = "medium string.".repeat(16).into() };

        // Use a long string which will hit the blob store.
        read_column_long_string { AlgebraicType::String => Box<str> = "long string. ".repeat(2048).into() };

        read_sum_value_plain { AlgebraicType::simple_enum(["a", "b"].into_iter()) => SumValue = SumValue::new_simple(1) };
        read_sum_tag_plain { AlgebraicType::simple_enum(["a", "b"].into_iter()) => SumTag = SumTag(1) };
    }

    #[test]
    fn read_sum_tag_from_sum_with_payload() {
        let algebraic_type = AlgebraicType::sum([("a", AlgebraicType::U8), ("b", AlgebraicType::U16)]);

        let pool = PagePool::new_for_test();
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(ProductType::from([algebraic_type]));

        let val = SumValue::new(1, 42u16);
        let (_, row_ref) = table.insert(&pool, &mut blob_store, &product![val.clone()]).unwrap();

        assert_eq!(val.tag, row_ref.read_col::<SumTag>(0).unwrap().0);
    }
}
