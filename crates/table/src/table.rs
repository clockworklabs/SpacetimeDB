use super::{
    bflatn_from::serialize_row_from_page,
    bflatn_to::{write_row_to_pages, write_row_to_pages_bsatn, Error},
    blob_store::BlobStore,
    eq::eq_row_in_page,
    eq_to_pv::eq_row_in_page_to_pv,
    indexes::{Bytes, PageIndex, PageOffset, RowHash, RowPointer, Size, SquashedOffset, PAGE_DATA_SIZE},
    layout::{AlgebraicTypeLayout, RowTypeLayout},
    page::{FixedLenRowsIter, Page},
    page_pool::PagePool,
    pages::Pages,
    pointer_map::PointerMap,
    read_column::{ReadColumn, TypeError},
    row_hash::hash_row_in_page,
    row_type_visitor::{row_type_visitor, VarLenVisitorProgram},
    static_assert_size,
    static_bsatn_validator::{static_bsatn_validator, validate_bsatn, StaticBsatnValidator},
    static_layout::StaticLayout,
    table_index::{TableIndex, TableIndexPointIter, TableIndexRangeIter},
    var_len::VarLenMembers,
    MemoryUsage,
};
use core::ops::RangeBounds;
use core::{fmt, ptr};
use core::{
    hash::{Hash, Hasher},
    hint::unreachable_unchecked,
};
use derive_more::{Add, AddAssign, From, Sub, SubAssign};
use enum_as_inner::EnumAsInner;
use smallvec::SmallVec;
use spacetimedb_lib::{bsatn::DecodeError, de::DeserializeOwned};
use spacetimedb_primitives::{ColId, ColList, IndexId, SequenceId};
use spacetimedb_sats::{
    algebraic_value::ser::ValueSerializer,
    bsatn::{self, ser::BsatnError, ToBsatn},
    i256,
    product_value::InvalidFieldError,
    satn::Satn,
    ser::{Serialize, Serializer},
    u256, AlgebraicValue, ProductType, ProductValue,
};
use spacetimedb_schema::{
    def::IndexAlgorithm,
    schema::{IndexSchema, TableSchema},
    type_for_generate::PrimitiveType,
};
use std::{
    collections::{btree_map, BTreeMap},
    sync::Arc,
};
use thiserror::Error;

/// The number of bytes used by, added to, or removed from a [`Table`]'s share of a [`BlobStore`].
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, From, Add, Sub, AddAssign, SubAssign)]
pub struct BlobNumBytes(usize);

impl MemoryUsage for BlobNumBytes {}

pub type SeqIdList = SmallVec<[SequenceId; 4]>;
static_assert_size!(SeqIdList, 24);

/// A database table containing the row schema, the rows, and indices.
///
/// The table stores the rows into a page manager
/// and uses an internal map to ensure that no identical row is stored more than once.
#[derive(Debug, PartialEq, Eq)]
pub struct Table {
    /// Page manager and row layout grouped together, for `RowRef` purposes.
    inner: TableInner,
    /// Maps `RowHash -> [RowPointer]` where a [`RowPointer`] points into `pages`.
    /// A [`PointerMap`] is effectively a specialized unique index on all the columns.
    ///
    /// In tables without any other unique constraints,
    /// the pointer map is used to enforce set semantics,
    /// i.e. to prevent duplicate rows.
    /// If `self.indexes` contains at least one unique index,
    /// duplicate rows are impossible regardless, so this will be `None`.
    pointer_map: Option<PointerMap>,
    /// The indices associated with a set of columns of the table.
    pub indexes: BTreeMap<IndexId, TableIndex>,
    /// The schema of the table, from which the type, and other details are derived.
    pub schema: Arc<TableSchema>,
    /// `SquashedOffset::TX_STATE` or `SquashedOffset::COMMITTED_STATE`
    /// depending on whether this is a tx scratchpad table
    /// or a committed table.
    squashed_offset: SquashedOffset,
    /// Stores number of rows present in table.
    pub row_count: u64,
    /// Stores the sum total number of bytes that each blob object in the table occupies.
    ///
    /// Note that the [`HashMapBlobStore`] does ref-counting and de-duplication,
    /// but this sum will count an object each time its hash is mentioned, rather than just once.
    blob_store_bytes: BlobNumBytes,
    /// Indicates whether this is a scheduler table or not.
    ///
    /// This is an optimization to avoid checking the schema in e.g., `InstanceEnv::{insert, update}`.
    is_scheduler: bool,
}

/// The part of a `Table` concerned only with storing rows.
///
/// Separated from the "outer" parts of `Table`, especially the `indexes`,
/// so that `RowRef` can borrow only the `TableInner`,
/// while other mutable references to the `indexes` exist.
/// This is necessary because index insertions and deletions take a `RowRef` as an argument,
/// from which they [`ReadColumn::read_column`] their keys.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TableInner {
    /// The type of rows this table stores, with layout information included.
    row_layout: RowTypeLayout,
    /// A [`StaticLayout`] for fast BFLATN <-> BSATN conversion,
    /// if the [`RowTypeLayout`] has a static BSATN length and layout.
    ///
    /// A [`StaticBsatnValidator`] is also included.
    /// It's used to validate BSATN-encoded rows before converting to BFLATN.
    static_layout: Option<(StaticLayout, StaticBsatnValidator)>,
    /// The visitor program for `row_layout`.
    ///
    /// Must be in the `TableInner` so that [`RowRef::blob_store_bytes`] can use it.
    visitor_prog: VarLenVisitorProgram,
    /// The page manager that holds rows
    /// including both their fixed and variable components.
    pages: Pages,
}

impl TableInner {
    /// Assumes `ptr` is a present row in `self` and returns a [`RowRef`] to it.
    ///
    /// # Safety
    ///
    /// The requirement is that `table.is_row_present(ptr)` must hold,
    /// where `table` is the `Table` which contains this `TableInner`.
    /// That is, `ptr` must refer to a row within `self`
    /// which was previously inserted and has not been deleted since.
    ///
    /// This means:
    /// - The `PageIndex` of `ptr` must be in-bounds for `self.pages`.
    /// - The `PageOffset` of `ptr` must be properly aligned for the row type of `self`,
    ///   and must refer to a valid, live row in that page.
    /// - The `SquashedOffset` of `ptr` must match the enclosing table's `table.squashed_offset`.
    ///
    /// Showing that `ptr` was the result of a call to [`Table::insert(table, ..)`]
    /// and has not been passed to [`Table::delete_internal_skip_pointer_map(table, ..)`]
    /// is sufficient to demonstrate all of these properties.
    unsafe fn get_row_ref_unchecked<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        squashed_offset: SquashedOffset,
        ptr: RowPointer,
    ) -> RowRef<'a> {
        // SAFETY: Forward caller requirements.
        unsafe { RowRef::new(self, blob_store, squashed_offset, ptr) }
    }

    /// Returns whether the row at `ptr` is present or not.
    // TODO: Remove all uses of this method,
    //       or more likely, gate them behind `debug_assert!`
    //       so they don't have semantic meaning.
    //
    //       Unlike the previous `locking_tx_datastore::Table`'s `RowId`,
    //       `RowPointer` is not content-addressed.
    //       This means it is possible to:
    //       - have a `RowPointer` A* to row A,
    //       - Delete row A,
    //       - Insert row B into the same storage as freed from A,
    //       - Test `is_row_present(A*)`, which falsely reports that row A is still present.
    //
    //       In the final interface, this method is superfluous anyways,
    //       as `RowPointer` is not part of our public interface.
    //       Instead, we will always discover a known-present `RowPointer`
    //       during a table scan or index seek.
    //       As such, our `delete` and `insert` methods can be `unsafe`
    //       and trust that the `RowPointer` is valid.
    fn is_row_present(&self, _squashed_offset: SquashedOffset, ptr: RowPointer) -> bool {
        if _squashed_offset != ptr.squashed_offset() {
            return false;
        }
        let Some((page, offset)) = self.try_page_and_offset(ptr) else {
            return false;
        };
        page.has_row_offset(self.row_layout.size(), offset)
    }

    fn try_page_and_offset(&self, ptr: RowPointer) -> Option<(&Page, PageOffset)> {
        (ptr.page_index().idx() < self.pages.len()).then(|| (&self.pages[ptr.page_index()], ptr.page_offset()))
    }

    /// Returns the page and page offset that `ptr` points to.
    fn page_and_offset(&self, ptr: RowPointer) -> (&Page, PageOffset) {
        self.try_page_and_offset(ptr).unwrap()
    }
}

static_assert_size!(Table, 264);

impl MemoryUsage for Table {
    fn heap_usage(&self) -> usize {
        let Self {
            inner,
            pointer_map,
            indexes,
            // MEMUSE: intentionally ignoring schema
            schema: _,
            squashed_offset,
            row_count,
            blob_store_bytes,
            is_scheduler,
        } = self;
        inner.heap_usage()
            + pointer_map.heap_usage()
            + indexes.heap_usage()
            + squashed_offset.heap_usage()
            + row_count.heap_usage()
            + blob_store_bytes.heap_usage()
            + is_scheduler.heap_usage()
    }
}

impl MemoryUsage for TableInner {
    fn heap_usage(&self) -> usize {
        let Self {
            row_layout,
            static_layout,
            visitor_prog,
            pages,
        } = self;
        row_layout.heap_usage() + static_layout.heap_usage() + visitor_prog.heap_usage() + pages.heap_usage()
    }
}

/// There was already a row with the same value.
#[derive(Error, Debug, PartialEq, Eq)]
#[error("Duplicate insertion of row {0:?} violates set semantics")]
pub struct DuplicateError(pub RowPointer);

/// Various error that can happen on table insertion.
#[derive(Error, Debug, PartialEq, Eq, EnumAsInner)]
pub enum InsertError {
    /// There was already a row with the same value.
    #[error(transparent)]
    Duplicate(#[from] DuplicateError),

    /// Couldn't write the row to the page manager.
    #[error(transparent)]
    Bflatn(#[from] super::bflatn_to::Error),

    /// Some index related error occurred.
    #[error(transparent)]
    IndexError(#[from] UniqueConstraintViolation),
}

/// Errors that can occur while trying to read a value via bsatn.
#[derive(Error, Debug)]
pub enum ReadViaBsatnError {
    #[error(transparent)]
    BSatnError(#[from] BsatnError),

    #[error(transparent)]
    DecodeError(#[from] DecodeError),
}

// Public API:
impl Table {
    /// Creates a new empty table with the given `schema` and `squashed_offset`.
    pub fn new(schema: Arc<TableSchema>, squashed_offset: SquashedOffset) -> Self {
        let row_layout: RowTypeLayout = schema.get_row_type().clone().into();
        let static_layout = StaticLayout::for_row_type(&row_layout).map(|sl| (sl, static_bsatn_validator(&row_layout)));
        let visitor_prog = row_type_visitor(&row_layout);
        // By default, we start off with an empty pointer map,
        // which is removed when the first unique index is added.
        let pm = Some(PointerMap::default());
        Self::new_raw(schema, row_layout, static_layout, visitor_prog, squashed_offset, pm)
    }

    /// Returns whether this is a scheduler table.
    pub fn is_scheduler(&self) -> bool {
        self.is_scheduler
    }

    /// Check if the `row` conflicts with any unique index on `self`,
    /// and if there is a conflict, return `Err`.
    ///
    /// `is_deleted` is a predicate which, for a given row pointer,
    /// returns true if and only if that row should be ignored.
    /// While checking unique constraints against the committed state,
    /// `MutTxId::insert` will ignore rows which are listed in the delete table.
    ///
    /// # Safety
    ///
    /// `row.row_layout() == self.row_layout()` must hold.
    pub unsafe fn check_unique_constraints<'a, I: Iterator<Item = (&'a IndexId, &'a TableIndex)>>(
        &'a self,
        row: RowRef<'_>,
        adapt: impl FnOnce(btree_map::Iter<'a, IndexId, TableIndex>) -> I,
        mut is_deleted: impl FnMut(RowPointer) -> bool,
    ) -> Result<(), UniqueConstraintViolation> {
        for (&index_id, index) in adapt(self.indexes.iter()).filter(|(_, index)| index.is_unique()) {
            // SAFETY: Caller promised that `row´ has the same layout as `self`.
            // Thus, as `index.indexed_columns` is in-bounds of `self`'s layout,
            // it's also in-bounds of `row`'s layout.
            let value = unsafe { row.project_unchecked(&index.indexed_columns) };
            if index.seek_point(&value).next().is_some_and(|ptr| !is_deleted(ptr)) {
                return Err(self.build_error_unique(index, index_id, value));
            }
        }
        Ok(())
    }

    /// Insert a `row` into this table, storing its large var-len members in the `blob_store`.
    ///
    /// On success, returns the hash, if any, of the newly-inserted row,
    /// and a `RowRef` referring to the row.s
    /// The hash is only computed if this table has a [`PointerMap`],
    /// i.e., does not have any unique indexes.
    /// If the table has unique indexes,
    /// the returned `Option<RowHash>` will be `None`.
    ///
    /// When a row equal to `row` already exists in `self`,
    /// returns `InsertError::Duplicate(existing_row_pointer)`,
    /// where `existing_row_pointer` is a `RowPointer` which identifies the existing row.
    /// In this case, the duplicate is not inserted,
    /// but internal data structures may be altered in ways that affect performance and fragmentation.
    ///
    /// TODO(error-handling): describe errors from `write_row_to_pages` and return meaningful errors.
    pub fn insert<'a>(
        &'a mut self,
        pool: &PagePool,
        blob_store: &'a mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<(Option<RowHash>, RowRef<'a>), InsertError> {
        // Optimistically insert the `row` before checking any constraints
        // under the assumption that errors (unique constraint & set semantic violations) are rare.
        let (row_ref, blob_bytes) = self.insert_physically_pv(pool, blob_store, row)?;
        let row_ptr = row_ref.pointer();

        // Confirm the insertion, checking any constraints, removing the physical row on error.
        // SAFETY: We just inserted `ptr`, so it must be present.
        // Re. `CHECK_SAME_ROW = true`,
        // where `insert` is called, we are not dealing with transactions,
        // and we already know there cannot be a duplicate row error,
        // but we check just in case it isn't.
        let (hash, row_ptr) = unsafe { self.confirm_insertion::<true>(blob_store, row_ptr, blob_bytes) }?;
        // SAFETY: Per post-condition of `confirm_insertion`, `row_ptr` refers to a valid row.
        let row_ref = unsafe { self.get_row_ref_unchecked(blob_store, row_ptr) };
        Ok((hash, row_ref))
    }

    /// Physically inserts `row` into the page
    /// without inserting it logically into the pointer map.
    ///
    /// This is useful when we need to insert a row temporarily to get back a `RowPointer`.
    /// A call to this method should be followed by a call to [`delete_internal_skip_pointer_map`].
    pub fn insert_physically_pv<'a>(
        &'a mut self,
        pool: &PagePool,
        blob_store: &'a mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<(RowRef<'a>, BlobNumBytes), Error> {
        // SAFETY: `self.pages` is known to be specialized for `self.row_layout`,
        // as `self.pages` was constructed from `self.row_layout` in `Table::new`.
        let (ptr, blob_bytes) = unsafe {
            write_row_to_pages(
                pool,
                &mut self.inner.pages,
                &self.inner.visitor_prog,
                blob_store,
                &self.inner.row_layout,
                row,
                self.squashed_offset,
            )
        }?;
        // SAFETY: We just inserted `ptr`, so it must be present.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, ptr) };

        Ok((row_ref, blob_bytes))
    }

    /// Physically insert a `row`, encoded in BSATN, into this table,
    /// storing its large var-len members in the `blob_store`.
    ///
    /// On success, returns the hash of the newly-inserted row,
    /// and a `RowRef` referring to the row.
    ///
    /// This does not check for set semantic or unique constraints.
    ///
    /// This is also useful when we need to insert a row temporarily to get back a `RowPointer`.
    /// In this case, A call to this method should be followed by a call to [`delete_internal_skip_pointer_map`].
    ///
    /// When `row` is not valid BSATN at the table's row type,
    /// an error is returned and there will be nothing for the caller to revert.
    pub fn insert_physically_bsatn<'a>(
        &'a mut self,
        pool: &PagePool,
        blob_store: &'a mut dyn BlobStore,
        row: &[u8],
    ) -> Result<(RowRef<'a>, BlobNumBytes), Error> {
        // Got a static layout? => Use fast-path insertion.
        let (ptr, blob_bytes) = if let Some((static_layout, static_validator)) = self.inner.static_layout.as_ref() {
            // Before inserting, validate the row, ensuring type safety.
            // SAFETY: The `static_validator` was derived from the same row layout as the static layout.
            unsafe { validate_bsatn(static_validator, static_layout, row) }.map_err(Error::Decode)?;

            let fixed_row_size = self.inner.row_layout.size();
            let squashed_offset = self.squashed_offset;
            let res = self
                .inner
                .pages
                .with_page_to_insert_row(pool, fixed_row_size, 0, |page| {
                    // SAFETY: We've used the right `row_size` and we trust that others have too.
                    // `RowTypeLayout` also ensures that we satisfy the minimum row size.
                    let fixed_offset = unsafe { page.alloc_fixed_len(fixed_row_size) }.map_err(Error::PageError)?;
                    let (mut fixed, _) = page.split_fixed_var_mut();
                    let fixed_buf = fixed.get_row_mut(fixed_offset, fixed_row_size);
                    // SAFETY:
                    // - We've validated that `row` is of sufficient length.
                    // - The `fixed_buf` is exactly the right `fixed_row_size`.
                    unsafe { static_layout.deserialize_row_into(fixed_buf, row) };
                    Ok(fixed_offset)
                })
                .map_err(Error::PagesError)?;
            match res {
                (page, Ok(offset)) => (RowPointer::new(false, page, offset, squashed_offset), 0.into()),
                (_, Err(e)) => return Err(e),
            }
        } else {
            // SAFETY: `self.pages` is known to be specialized for `self.row_layout`,
            // as `self.pages` was constructed from `self.row_layout` in `Table::new`.
            unsafe {
                write_row_to_pages_bsatn(
                    pool,
                    &mut self.inner.pages,
                    &self.inner.visitor_prog,
                    blob_store,
                    &self.inner.row_layout,
                    row,
                    self.squashed_offset,
                )
            }?
        };

        // SAFETY: We just inserted `ptr`, so it must be present.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, ptr) };

        Ok((row_ref, blob_bytes))
    }

    /// Returns all the columns with sequences that need generation for this `row`.
    ///
    /// # Safety
    ///
    /// `self.is_row_present(row)` must hold.
    pub unsafe fn sequence_triggers_for<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        row: RowPointer,
    ) -> (ColList, SeqIdList) {
        let sequences = &*self.get_schema().sequences;
        let row_ty = self.row_layout().product();

        // SAFETY: Caller promised that `self.is_row_present(row)` holds.
        let row_ref = unsafe { self.get_row_ref_unchecked(blob_store, row) };

        sequences
            .iter()
            // Find all the sequences that are triggered by this row.
            .filter(|seq| {
                // SAFETY: `seq.col_pos` is in-bounds of `row_ty.elements`
                // as `row_ty` was derived from the same schema as `seq` is part of.
                let elem_ty = unsafe { &row_ty.elements.get_unchecked(seq.col_pos.idx()) };
                // SAFETY:
                // - `elem_ty` appears as a column in the row type.
                // - `AlgebraicValue` is compatible with all types.
                let val = unsafe { AlgebraicValue::unchecked_read_column(row_ref, elem_ty) };
                val.is_numeric_zero()
            })
            .map(|seq| (seq.col_pos, seq.sequence_id))
            .unzip()
    }

    /// Writes `seq_val` to the column at `col_id` in the row identified by `ptr`.
    ///
    /// Truncates the `seq_val` to fit the type of the column.
    ///
    /// # Safety
    ///
    /// - `self.is_row_present(row)` must hold.
    /// - `col_id` must be a valid column, with a primitive integer type, of the row type.
    pub unsafe fn write_gen_val_to_col(&mut self, col_id: ColId, ptr: RowPointer, seq_val: i128) {
        let row_ty = self.inner.row_layout.product();
        // SAFETY: Caller promised that `col_id` was a valid column.
        let elem_ty = unsafe { row_ty.elements.get_unchecked(col_id.idx()) };
        let AlgebraicTypeLayout::Primitive(col_typ) = elem_ty.ty else {
            // SAFETY: Columns with sequences must be primitive types.
            unsafe { unreachable_unchecked() }
        };

        let fixed_row_size = self.inner.row_layout.size();
        let fixed_buf = self.inner.pages[ptr.page_index()].get_fixed_row_data_mut(ptr.page_offset(), fixed_row_size);

        fn write<const N: usize>(dst: &mut [u8], offset: u16, bytes: [u8; N]) {
            let offset = offset as usize;
            dst[offset..offset + N].copy_from_slice(&bytes);
        }

        match col_typ {
            PrimitiveType::I8 => write(fixed_buf, elem_ty.offset, (seq_val as i8).to_le_bytes()),
            PrimitiveType::U8 => write(fixed_buf, elem_ty.offset, (seq_val as u8).to_le_bytes()),
            PrimitiveType::I16 => write(fixed_buf, elem_ty.offset, (seq_val as i16).to_le_bytes()),
            PrimitiveType::U16 => write(fixed_buf, elem_ty.offset, (seq_val as u16).to_le_bytes()),
            PrimitiveType::I32 => write(fixed_buf, elem_ty.offset, (seq_val as i32).to_le_bytes()),
            PrimitiveType::U32 => write(fixed_buf, elem_ty.offset, (seq_val as u32).to_le_bytes()),
            PrimitiveType::I64 => write(fixed_buf, elem_ty.offset, (seq_val as i64).to_le_bytes()),
            PrimitiveType::U64 => write(fixed_buf, elem_ty.offset, (seq_val as u64).to_le_bytes()),
            PrimitiveType::I128 => write(fixed_buf, elem_ty.offset, seq_val.to_le_bytes()),
            PrimitiveType::U128 => write(fixed_buf, elem_ty.offset, (seq_val as u128).to_le_bytes()),
            PrimitiveType::I256 => write(fixed_buf, elem_ty.offset, (i256::from(seq_val)).to_le_bytes()),
            PrimitiveType::U256 => write(fixed_buf, elem_ty.offset, (u256::from(seq_val as u128)).to_le_bytes()),
            // SAFETY: Columns with sequences must be integer types.
            PrimitiveType::Bool | PrimitiveType::F32 | PrimitiveType::F64 => unsafe { unreachable_unchecked() },
        }
    }

    /// Performs all the checks necessary after having fully decided on a rows contents.
    ///
    /// This includes inserting the row into any applicable indices and/or the pointer map.
    ///
    /// On `Ok(_)`, statistics of the table are also updated,
    /// and the `ptr` still points to a valid row, and otherwise not.
    ///
    /// If `CHECK_SAME_ROW` holds, an identical row will be treated as a set-semantic duplicate.
    /// Otherwise, it will be treated as a unique constraint violation.
    /// However, `false` should only be passed if it's known beforehand that there is no identical row.
    ///
    /// # Safety
    ///
    /// `self.is_row_present(row)` must hold.
    pub unsafe fn confirm_insertion<'a, const CHECK_SAME_ROW: bool>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        ptr: RowPointer,
        blob_bytes: BlobNumBytes,
    ) -> Result<(Option<RowHash>, RowPointer), InsertError> {
        // SAFETY: Caller promised that `self.is_row_present(ptr)` holds.
        let hash = unsafe { self.insert_into_pointer_map(blob_store, ptr) }?;
        // SAFETY: Caller promised that `self.is_row_present(ptr)` holds.
        unsafe { self.insert_into_indices::<CHECK_SAME_ROW>(blob_store, ptr) }?;

        self.update_statistics_added_row(blob_bytes);
        Ok((hash, ptr))
    }

    /// Confirms a row update, after first updating indices and checking constraints.
    ///
    /// On `Ok(_)`:
    /// - the statistics of the table are also updated,
    /// - the `ptr` still points to a valid row.
    ///
    /// Otherwise, on `Err(_)`:
    /// - `ptr` will not point to a valid row,
    /// - the statistics won't be updated.
    ///
    /// # Safety
    ///
    /// `self.is_row_present(new_row)` and `self.is_row_present(old_row)`  must hold.
    pub unsafe fn confirm_update<'a>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        new_ptr: RowPointer,
        old_ptr: RowPointer,
        blob_bytes_added: BlobNumBytes,
    ) -> Result<RowPointer, InsertError> {
        // (1) Remove old row from indices.
        // SAFETY: Caller promised that `self.is_row_present(old_ptr)` holds.
        unsafe { self.delete_from_indices(blob_store, old_ptr) };

        // Insert new row into indices.
        // SAFETY: Caller promised that `self.is_row_present(ptr)` holds.
        let res = unsafe { self.insert_into_indices::<true>(blob_store, new_ptr) };
        if let Err(e) = res {
            // Undo (1).
            unsafe { self.insert_into_indices::<true>(blob_store, old_ptr) }
                .expect("re-inserting the old row into indices should always work");
            return Err(e);
        }

        // Remove the old row physically.
        // SAFETY: The physical `old_ptr` still exists.
        let blob_bytes_removed = unsafe { self.delete_internal_skip_pointer_map(blob_store, old_ptr) };
        self.update_statistics_deleted_row(blob_bytes_removed);

        // Update statistics.
        self.update_statistics_added_row(blob_bytes_added);
        Ok(new_ptr)
    }

    /// We've added a row, update the statistics to record this.
    #[inline]
    fn update_statistics_added_row(&mut self, blob_bytes: BlobNumBytes) {
        self.row_count += 1;
        self.blob_store_bytes += blob_bytes;
    }

    /// We've removed a row, update the statistics to record this.
    #[inline]
    fn update_statistics_deleted_row(&mut self, blob_bytes: BlobNumBytes) {
        self.row_count -= 1;
        self.blob_store_bytes -= blob_bytes;
    }

    /// Insert row identified by `new` into indices.
    /// This also checks unique constraints.
    /// Deletes the row if there were any violations.
    ///
    /// If `CHECK_SAME_ROW`, upon a unique constraint violation,
    /// this will check if it's really a duplicate row.
    /// Otherwise, the unique constraint violation is returned.
    ///
    /// SAFETY: `self.is_row_present(new)` must hold.
    /// Post-condition: If this method returns `Ok(_)`, the row still exists.
    unsafe fn insert_into_indices<'a, const CHECK_SAME_ROW: bool>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        new: RowPointer,
    ) -> Result<(), InsertError> {
        self.indexes
            .iter_mut()
            .try_for_each(|(index_id, index)| {
                // SAFETY: We just inserted `ptr`, so it must be present.
                let new = unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, new) };
                // SAFETY: any index in this table was constructed with the same row type as this table.
                let violation = unsafe { index.check_and_insert(new) };
                violation.map_err(|old| (*index_id, old, new))
            })
            .map_err(|(index_id, old, new)| {
                // Found unique constraint violation!
                if CHECK_SAME_ROW
                    // If the index was added in this tx,
                    // `old` could be a committed row,
                    // which we want to avoid here.
                    // TODO(centril): not 100% correct, could still be a duplicate,
                    // but this is rather pathological and should be fixed when we restructure.
                    && old.squashed_offset().is_tx_state()
                    // SAFETY:
                    // - The row layouts are the same as it's the same table.
                    // - We know `old` exists in `self` as we just found it in an index.
                    // - Caller promised that `new` is valid for `self`.
                    && unsafe { Self::eq_row_in_page(self, old, self, new.pointer()) }
                {
                    return (index_id, DuplicateError(old).into());
                }

                let index = self.indexes.get(&index_id).unwrap();
                let value = new.project(&index.indexed_columns).unwrap();
                let error = self.build_error_unique(index, index_id, value).into();
                (index_id, error)
            })
            .map_err(|(index_id, error)| {
                // Delete row from indices.
                // Do this before the actual deletion, as `index.delete` needs a `RowRef`
                // so it can extract the appropriate value.
                // SAFETY: We just inserted `new`, so it must be present.
                unsafe { self.delete_from_indices_until(blob_store, new, index_id) };

                // Cleanup, undo the row insertion of `new`s.
                // SAFETY: We just inserted `new`, so it must be present.
                unsafe { self.delete_internal(blob_store, new) };

                error
            })
    }

    /// Finds the [`RowPointer`] to the row in `target_table` equal, if any,
    /// to the row `needle_ptr` in `needle_table`,
    /// by any unique index in `target_table`.
    ///
    /// # Safety
    ///
    /// - `target_table` and `needle_table` must have the same `row_layout`.
    /// - `needle_table.is_row_present(needle_ptr)` must hold.
    unsafe fn find_same_row_via_unique_index(
        target_table: &Table,
        needle_table: &Table,
        needle_bs: &dyn BlobStore,
        needle_ptr: RowPointer,
    ) -> Option<RowPointer> {
        // Use some index (the one with the lowest `IndexId` currently).
        // TODO(centril): this isn't what we actually want.
        // Rather, we'd prefer the index with the simplest type,
        // but this is left as future work as we don't have to optimize this method now.
        let target_index = target_table
            .indexes
            .values()
            .find(|idx| idx.is_unique())
            .expect("there should be at least one unique index");
        // Project the needle row to the columns of the index, and then seek.
        // As this is a unique index, there are 0-1 rows for this key.
        let needle_row = unsafe { needle_table.get_row_ref_unchecked(needle_bs, needle_ptr) };
        let key = needle_row
            .project(&target_index.indexed_columns)
            .expect("needle row should be valid");
        target_index.seek_point(&key).next().filter(|&target_ptr| {
            // SAFETY:
            // - Caller promised that the row layouts were the same.
            // - We know `target_ptr` exists, as it was in `target_index`, belonging to `target_table`.
            // - Caller promised that `needle_ptr` is valid for `needle_table`.
            unsafe { Self::eq_row_in_page(target_table, target_ptr, needle_table, needle_ptr) }
        })
    }

    /// Insert the row identified by `ptr` into the table's [`PointerMap`],
    /// if the table has one.
    ///
    /// This checks for set semantic violations.
    /// If a set semantic conflict (i.e. duplicate row) is detected by the pointer map,
    /// the row will be deleted and an error returned.
    /// If the pointer map confirms that the row was unique, returns the `RowHash` of that row.
    ///
    /// If this table has no `PointerMap`, returns `Ok(None)`.
    /// In that case, the row's uniqueness will be verified by [`Self::insert_into_indices`],
    /// as this table has at least one unique index.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    /// Post-condition: If this method returns `Ok(_)`, the row still exists.
    unsafe fn insert_into_pointer_map<'a>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        ptr: RowPointer,
    ) -> Result<Option<RowHash>, DuplicateError> {
        if self.pointer_map.is_none() {
            // No pointer map? Set semantic constraint is checked by a unique index instead.
            return Ok(None);
        };

        // SAFETY:
        // - `self` trivially has the same `row_layout` as `self`.
        // - Caller promised that `self.is_row_present(row)` holds.
        let (hash, existing_row) = unsafe { Self::find_same_row_via_pointer_map(self, self, blob_store, ptr, None) };

        if let Some(existing_row) = existing_row {
            // If an equal row was already present,
            // roll back our optimistic insert to avoid violating set semantics.

            // SAFETY: Caller promised that `ptr` is a valid row in `self`.
            unsafe {
                self.inner
                    .pages
                    .delete_row(&self.inner.visitor_prog, self.row_size(), ptr, blob_store)
            };
            return Err(DuplicateError(existing_row));
        }

        // If the optimistic insertion was correct,
        // i.e. this is not a set-semantic duplicate,
        // add it to the `pointer_map`.
        self.pointer_map
            .as_mut()
            .expect("pointer map should exist, as it did previously")
            .insert(hash, ptr);

        Ok(Some(hash))
    }

    /// Returns the list of pointers to rows which hash to `row_hash`.
    ///
    /// If `self` does not have a [`PointerMap`], always returns the empty slice.
    fn pointers_for(&self, row_hash: RowHash) -> &[RowPointer] {
        self.pointer_map.as_ref().map_or(&[], |pm| pm.pointers_for(row_hash))
    }

    /// Using the [`PointerMap`],
    /// searches `target_table` for a row equal to `needle_table[needle_ptr]`.
    ///
    /// Rows are compared for equality by [`eq_row_in_page`].
    ///
    /// Lazily computes the row hash if needed and returns it, or uses the one provided, if any.
    ///
    /// Used for detecting set-semantic duplicates when inserting
    /// into tables without any unique constraints.
    ///
    /// Does nothing and always returns `None` if `target_table` does not have a `PointerMap`,
    /// in which case the caller should instead use [`Self::find_same_row_via_unique_index`].
    ///
    /// Note that we don't need the blob store to compute equality,
    /// as content-addressing means it's sufficient to compare the hashes of large blobs.
    /// (If we see a collision in `BlobHash` we have bigger problems.)
    ///
    /// # Safety
    ///
    /// - `target_table` and `needle_table` must have the same `row_layout`.
    /// - `needle_table.is_row_present(needle_ptr)`.
    pub unsafe fn find_same_row_via_pointer_map(
        target_table: &Table,
        needle_table: &Table,
        needle_bs: &dyn BlobStore,
        needle_ptr: RowPointer,
        row_hash: Option<RowHash>,
    ) -> (RowHash, Option<RowPointer>) {
        let row_hash = row_hash.unwrap_or_else(|| {
            // SAFETY: Caller promised that `needle_table.is_row_present(needle_ptr)`.
            let row_ref = unsafe { needle_table.get_row_ref_unchecked(needle_bs, needle_ptr) };
            row_ref.row_hash()
        });

        // Scan all the frow pointers with `row_hash` in the `committed_table`.
        let row_ptr = target_table.pointers_for(row_hash).iter().copied().find(|&target_ptr| {
            // SAFETY:
            // - Caller promised that the row layouts were the same.
            // - We know `target_ptr` exists, as it was found in a pointer map.
            // - Caller promised that `needle_ptr` is valid for `needle_table`.
            unsafe { Self::eq_row_in_page(target_table, target_ptr, needle_table, needle_ptr) }
        });

        (row_hash, row_ptr)
    }

    /// Returns whether the row `target_ptr` in `target_table`
    /// is exactly equal to the row `needle_ptr` in `needle_ptr`.
    ///
    /// # Safety
    ///
    /// - `target_table` and `needle_table` must have the same `row_layout`.
    /// - `target_table.is_row_present(target_ptr)`.
    /// - `needle_table.is_row_present(needle_ptr)`.
    pub unsafe fn eq_row_in_page(
        target_table: &Table,
        target_ptr: RowPointer,
        needle_table: &Table,
        needle_ptr: RowPointer,
    ) -> bool {
        let (target_page, target_offset) = target_table.inner.page_and_offset(target_ptr);
        let (needle_page, needle_offset) = needle_table.inner.page_and_offset(needle_ptr);

        // SAFETY:
        // - Caller promised that `target_ptr` is valid, so `target_page` and `target_offset` are both valid.
        // - Caller promised that `needle_ptr` is valid, so `needle_page` and `needle_offset` are both valid.
        // - Caller promised that the layouts of `target_table` and `needle_table` are the same,
        //   so `target_table` applies to both.
        //   Moreover `(x: Table).inner.static_layout` is always derived from `x.row_layout`.
        unsafe {
            eq_row_in_page(
                target_page,
                needle_page,
                target_offset,
                needle_offset,
                &target_table.inner.row_layout,
                target_table.static_layout(),
            )
        }
    }

    /// Searches `target_table` for a row equal to `needle_table[needle_ptr]`,
    /// and returns the [`RowPointer`] to that row in `target_table`, if it exists.
    ///
    /// Searches using the [`PointerMap`] or a unique index, as appropriate for the table.
    ///
    /// Lazily computes the row hash if needed and returns it, or uses the one provided, if any.
    ///
    /// # Safety
    ///
    /// - `target_table` and `needle_table` must have the same `row_layout`.
    /// - `needle_table.is_row_present(needle_ptr)` must hold.
    pub unsafe fn find_same_row(
        target_table: &Table,
        needle_table: &Table,
        needle_bs: &dyn BlobStore,
        needle_ptr: RowPointer,
        row_hash: Option<RowHash>,
    ) -> (Option<RowHash>, Option<RowPointer>) {
        if target_table.pointer_map.is_some() {
            // SAFETY: Caller promised that `target_table` and `needle_table` have the same `row_layout`.
            // SAFETY: Caller promised that `needle_table.is_row_present(needle_ptr)`.
            let (row_hash, row_ptr) = unsafe {
                Self::find_same_row_via_pointer_map(target_table, needle_table, needle_bs, needle_ptr, row_hash)
            };
            (Some(row_hash), row_ptr)
        } else {
            (
                row_hash,
                // SAFETY: Caller promised that `target_table` and `needle_table` have the same `row_layout`.
                // SAFETY: Caller promised that `needle_table.is_row_present(needle_ptr)`.
                unsafe { Self::find_same_row_via_unique_index(target_table, needle_table, needle_bs, needle_ptr) },
            )
        }
    }

    /// Returns a [`RowRef`] for `ptr` or `None` if the row isn't present.
    pub fn get_row_ref<'a>(&'a self, blob_store: &'a dyn BlobStore, ptr: RowPointer) -> Option<RowRef<'a>> {
        self.is_row_present(ptr)
            // SAFETY: We only call `get_row_ref_unchecked` when `is_row_present` holds.
            .then(|| unsafe { self.get_row_ref_unchecked(blob_store, ptr) })
    }

    /// Assumes `ptr` is a present row in `self` and returns a [`RowRef`] to it.
    ///
    /// # Safety
    ///
    /// The requirement is that `self.is_row_present(ptr)` must hold.
    /// That is, `ptr` must refer to a row within `self`
    /// which was previously inserted and has not been deleted since.
    ///
    /// This means:
    /// - The `PageIndex` of `ptr` must be in-bounds for `self.pages`.
    /// - The `PageOffset` of `ptr` must be properly aligned for the row type of `self`,
    ///   and must refer to a valid, live row in that page.
    /// - The `SquashedOffset` of `ptr` must match `self.squashed_offset`.
    ///
    /// Showing that `ptr` was the result of a call to [`Table::insert(table, ..)`]
    /// and has not been passed to [`Table::delete(table, ..)`]
    /// is sufficient to demonstrate all of these properties.
    pub unsafe fn get_row_ref_unchecked<'a>(&'a self, blob_store: &'a dyn BlobStore, ptr: RowPointer) -> RowRef<'a> {
        // SAFETY: Caller promised that ^-- holds.
        unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, ptr) }
    }

    /// Deletes a row in the page manager
    /// without deleting it logically in the pointer map.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid, live row in this table.
    pub unsafe fn delete_internal_skip_pointer_map(
        &mut self,
        blob_store: &mut dyn BlobStore,
        ptr: RowPointer,
    ) -> BlobNumBytes {
        debug_assert!(self.is_row_present(ptr));
        // Delete the physical row.
        //
        // SAFETY:
        // - `ptr` points to a valid row in this table, per our invariants.
        // - `self.row_size` known to be consistent with `self.pages`,
        //    as the two are tied together in `Table::new`.
        unsafe {
            self.inner
                .pages
                .delete_row(&self.inner.visitor_prog, self.row_size(), ptr, blob_store)
        }
    }

    /// Deletes the row identified by `ptr` from the table.
    ///
    /// Returns the number of blob bytes added. This method does not update statistics by itself.
    ///
    /// NOTE: This method skips updating indexes.
    /// Use `delete_unchecked` or `delete` to delete a row with index updating.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    unsafe fn delete_internal(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) -> BlobNumBytes {
        // Remove the set semantic association.
        if let Some(pointer_map) = &mut self.pointer_map {
            // SAFETY: `self.is_row_present(row)` holds.
            let row = unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, ptr) };

            let _remove_result = pointer_map.remove(row.row_hash(), ptr);
            debug_assert!(_remove_result);
        }

        // Delete the physical row.
        // SAFETY: `ptr` points to a valid row in this table as `self.is_row_present(row)` holds.
        unsafe { self.delete_internal_skip_pointer_map(blob_store, ptr) }
    }

    /// Deletes the row identified by `ptr` from the table.
    ///
    /// Returns the number of blob bytes deleted. This method does not update statistics by itself.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    unsafe fn delete_unchecked(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) -> BlobNumBytes {
        // Delete row from indices.
        // Do this before the actual deletion, as `index.delete` needs a `RowRef`
        // so it can extract the appropriate value.
        // SAFETY: Caller promised that `self.is_row_present(row)` holds.
        unsafe { self.delete_from_indices(blob_store, ptr) };

        // SAFETY: Caller promised that `self.is_row_present(row)` holds.
        unsafe { self.delete_internal(blob_store, ptr) }
    }

    /// Delete `row_ref` from all the indices of this table until `index_id` is reached.
    /// The range is exclusive of `index_id`.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    unsafe fn delete_from_indices_until(&mut self, blob_store: &dyn BlobStore, ptr: RowPointer, index_id: IndexId) {
        // SAFETY: Caller promised that `self.is_row_present(row)` holds.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, ptr) };

        for (_, index) in self.indexes.range_mut(..index_id) {
            index.delete(row_ref).unwrap();
        }
    }

    /// Delete `row_ref` from all the indices of this table.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    unsafe fn delete_from_indices(&mut self, blob_store: &dyn BlobStore, ptr: RowPointer) {
        // SAFETY: Caller promised that `self.is_row_present(row)` holds.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, self.squashed_offset, ptr) };

        for index in self.indexes.values_mut() {
            index.delete(row_ref).unwrap();
        }
    }

    /// Deletes the row identified by `ptr` from the table.
    ///
    /// The function `before` is run on the to-be-deleted row,
    /// if it is present, before deleting.
    /// This enables callers to extract the deleted row.
    /// E.g. applying deletes when squashing/merging a transaction into the committed state
    /// passes `|row| row.to_product_value()` as `before`
    /// so that the resulting `ProductValue`s can be passed to the subscription evaluator.
    pub fn delete<'a, R>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        ptr: RowPointer,
        before: impl for<'b> FnOnce(RowRef<'b>) -> R,
    ) -> Option<R> {
        if !self.is_row_present(ptr) {
            return None;
        };

        // SAFETY: We only call `get_row_ref_unchecked` when `is_row_present` holds.
        let row_ref = unsafe { self.get_row_ref_unchecked(blob_store, ptr) };

        let ret = before(row_ref);

        // SAFETY: We've checked above that `self.is_row_present(ptr)`.
        let blob_bytes_deleted = unsafe { self.delete_unchecked(blob_store, ptr) };
        self.update_statistics_deleted_row(blob_bytes_deleted);

        Some(ret)
    }

    /// If a row exists in `self` which matches `row`
    /// by [`Table::find_same_row`],
    /// delete that row.
    ///
    /// If a matching row was found, returns the pointer to that row.
    /// The returned pointer is now invalid, as the row to which it referred has been deleted.
    ///
    /// This operation works by temporarily inserting the `row` into `self`,
    /// checking `find_same_row` on the newly-inserted row,
    /// deleting the matching row if it exists,
    /// then deleting the temporary insertion.
    pub fn delete_equal_row(
        &mut self,
        pool: &PagePool,
        blob_store: &mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<Option<RowPointer>, Error> {
        // Insert `row` temporarily so `temp_ptr` and `hash` can be used to find the row.
        // This must avoid consulting and inserting to the pointer map,
        // as the row is already present, set-semantically.
        let (temp_row, _) = self.insert_physically_pv(pool, blob_store, row)?;
        let temp_ptr = temp_row.pointer();

        // Find the row equal to the passed-in `row`.
        // This uses one of two approaches.
        // Either there is a pointer map, so we use that,
        // or, here is at least one unique index, so we use one of them.
        //
        // SAFETY:
        // - `self` trivially has the same `row_layout` as `self`.
        // - We just inserted `temp_ptr`, so it's valid.
        let (_, existing_row_ptr) = unsafe { Self::find_same_row(self, self, blob_store, temp_ptr, None) };

        // If an equal row was present, delete it.
        if let Some(existing_row_ptr) = existing_row_ptr {
            let blob_bytes_deleted = unsafe {
                // SAFETY: `find_same_row` ensures that the pointer is valid.
                self.delete_unchecked(blob_store, existing_row_ptr)
            };
            self.update_statistics_deleted_row(blob_bytes_deleted);
        }

        // Remove the temporary row we inserted in the beginning.
        // Avoid the pointer map, since we don't want to delete it twice.
        // SAFETY: `ptr` is valid as we just inserted it.
        unsafe {
            self.delete_internal_skip_pointer_map(blob_store, temp_ptr);
        }

        Ok(existing_row_ptr)
    }

    /// Returns the row type for rows in this table.
    pub fn get_row_type(&self) -> &ProductType {
        self.get_schema().get_row_type()
    }

    /// Returns the schema for this table.
    pub fn get_schema(&self) -> &Arc<TableSchema> {
        &self.schema
    }

    /// Runs a mutation on the [`TableSchema`] of this table.
    ///
    /// This uses a clone-on-write mechanism.
    /// If none but `self` refers to the schema, then the mutation will be in-place.
    /// Otherwise, the schema must be cloned, mutated,
    /// and then the cloned version is written back to the table.
    pub fn with_mut_schema<R>(&mut self, with: impl FnOnce(&mut TableSchema) -> R) -> R {
        with(Arc::make_mut(&mut self.schema))
    }

    /// Returns a new [`TableIndex`] for `table`.
    pub fn new_index(&self, algo: &IndexAlgorithm, is_unique: bool) -> Result<TableIndex, InvalidFieldError> {
        TableIndex::new(self.get_schema().get_row_type(), algo, is_unique)
    }

    /// Inserts a new `index` into the table.
    ///
    /// The index will be populated using the rows of the table.
    ///
    /// # Panics
    ///
    /// Panics if any row would violate `index`'s unique constraint, if it has one.
    ///
    /// # Safety
    ///
    /// Caller must promise that `index` was constructed with the same row type/layout as this table.
    pub unsafe fn insert_index(&mut self, blob_store: &dyn BlobStore, index_id: IndexId, mut index: TableIndex) {
        let rows = self.scan_rows(blob_store);
        // SAFETY: Caller promised that table's row type/layout
        // matches that which `index` was constructed with.
        // It follows that this applies to any `rows`, as required.
        let violation = unsafe { index.build_from_rows(rows) };
        violation.unwrap_or_else(|ptr| {
            panic!("adding `index` should cause no unique constraint violations, but {ptr:?} would")
        });
        // SAFETY: Forward caller requirement.
        unsafe { self.add_index(index_id, index) };
    }

    /// Adds an index to the table without populating.
    ///
    /// # Safety
    ///
    /// Caller must promise that `index` was constructed with the same row type/layout as this table.
    pub unsafe fn add_index(&mut self, index_id: IndexId, index: TableIndex) -> Option<PointerMap> {
        let is_unique = index.is_unique();
        self.indexes.insert(index_id, index);

        // Remove the pointer map, if any.
        if is_unique {
            self.pointer_map.take()
        } else {
            None
        }
    }

    /// Removes an index from the table.
    ///
    /// Returns whether an index existed with `index_id`.
    pub fn delete_index(
        &mut self,
        blob_store: &dyn BlobStore,
        index_id: IndexId,
        pointer_map: Option<PointerMap>,
    ) -> Option<(TableIndex, IndexSchema)> {
        let index = self.indexes.remove(&index_id)?;

        // If we removed the last unique index, add a pointer map.
        if index.is_unique() && !self.indexes.values().any(|idx| idx.is_unique()) {
            self.pointer_map = Some(pointer_map.unwrap_or_else(|| self.rebuild_pointer_map(blob_store)));
        }

        // Remove index from schema.
        //
        // This likely will do a clone-write as over time?
        // The schema might have found other referents.
        let schema = self
            .with_mut_schema(|s| s.remove_index(index_id))
            .expect("there should be an index with `index_id`");
        Some((index, schema))
    }

    /// Returns an iterator over all the rows of `self`, yielded as [`RefRef`]s.
    pub fn scan_rows<'a>(&'a self, blob_store: &'a dyn BlobStore) -> TableScanIter<'a> {
        TableScanIter {
            current_page: None, // Will be filled by the iterator.
            current_page_idx: PageIndex(0),
            table: self,
            blob_store,
        }
    }

    /// Returns this table combined with the index for [`IndexId`], if any.
    pub fn get_index_by_id_with_table<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        index_id: IndexId,
    ) -> Option<TableAndIndex<'a>> {
        Some(TableAndIndex {
            table: self,
            blob_store,
            index: self.get_index_by_id(index_id)?,
        })
    }

    /// Returns the [`TableIndex`] for this [`IndexId`].
    pub fn get_index_by_id(&self, index_id: IndexId) -> Option<&TableIndex> {
        self.indexes.get(&index_id)
    }

    /// Returns this table combined with the first index with `cols`, if any.
    pub fn get_index_by_cols_with_table<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        cols: &ColList,
    ) -> Option<TableAndIndex<'a>> {
        let (_, index) = self.get_index_by_cols(cols)?;
        Some(TableAndIndex {
            table: self,
            blob_store,
            index,
        })
    }

    /// Returns the first [`TableIndex`] with the given [`ColList`].
    pub fn get_index_by_cols(&self, cols: &ColList) -> Option<(IndexId, &TableIndex)> {
        self.indexes
            .iter()
            .find(|(_, index)| &index.indexed_columns == cols)
            .map(|(id, idx)| (*id, idx))
    }

    /// Clones the structure of this table into a new one with
    /// the same schema, visitor program, and indices.
    /// The new table will be completely empty
    /// and will use the given `squashed_offset` instead of that of `self`.
    pub fn clone_structure(&self, squashed_offset: SquashedOffset) -> Self {
        // Clone a bunch of static data.
        // NOTE(centril): It's important that these be cheap to clone.
        // This is why they are all `Arc`ed or have some sort of small-vec optimization.
        let schema = self.schema.clone();
        let layout = self.row_layout().clone();
        let sbl = self.inner.static_layout.clone();
        let visitor = self.inner.visitor_prog.clone();

        // If we had a pointer map, we'll have one in the cloned one as well, but empty.
        let pm = self.pointer_map.as_ref().map(|_| PointerMap::default());

        // Make the new table.
        let mut new = Table::new_raw(schema, layout, sbl, visitor, squashed_offset, pm);

        // Clone the index structure. The table is empty, so no need to `build_from_rows`.
        for (&index_id, index) in self.indexes.iter() {
            new.indexes.insert(index_id, index.clone_structure());
        }
        new
    }

    /// Returns the number of bytes occupied by the pages and the blob store.
    /// Note that result can be more than the actual physical size occupied by the table
    /// because the blob store implementation can do internal optimizations.
    /// For more details, refer to the documentation of `self.blob_store_bytes`.
    pub fn bytes_occupied_overestimate(&self) -> usize {
        (self.num_pages() * PAGE_DATA_SIZE) + (self.blob_store_bytes.0)
    }

    /// Reset the internal storage of `self` to be `pages`.
    ///
    /// This recomputes the pointer map based on the `pages`,
    /// but does not recompute indexes.
    ///
    /// Used when restoring from a snapshot.
    ///
    /// # Safety
    ///
    /// The schema of rows stored in the `pages` must exactly match `self.schema` and `self.inner.row_layout`.
    pub unsafe fn set_pages(&mut self, pages: Vec<Box<Page>>, blob_store: &dyn BlobStore) {
        self.inner.pages.set_contents(pages, self.inner.row_layout.size());

        // Recompute table metadata based on the new pages.
        // Compute the row count first, in case later computations want to use it as a capacity to pre-allocate.
        self.compute_row_count(blob_store);
        self.pointer_map = Some(self.rebuild_pointer_map(blob_store));
    }

    /// Consumes the table, returning some constituents needed for merge.
    pub fn consume_for_merge(
        self,
    ) -> (
        Arc<TableSchema>,
        impl Iterator<Item = (IndexId, TableIndex)>,
        impl Iterator<Item = Box<Page>>,
    ) {
        (self.schema, self.indexes.into_iter(), self.inner.pages.into_page_iter())
    }

    /// Returns the number of rows resident in this table.
    ///
    /// This method runs in constant time.
    pub fn num_rows(&self) -> u64 {
        self.row_count
    }

    #[cfg(test)]
    fn reconstruct_num_rows(&self) -> u64 {
        self.pages().iter().map(|page| page.reconstruct_num_rows() as u64).sum()
    }

    /// Returns the number of bytes used by rows resident in this table.
    ///
    /// This includes data bytes, padding bytes and some overhead bytes,
    /// as described in the docs for [`Page::bytes_used_by_rows`],
    /// but *does not* include:
    ///
    /// - Unallocated space within pages.
    /// - Per-page overhead (e.g. page headers).
    /// - Table overhead (e.g. the [`RowTypeLayout`], [`PointerMap`], [`Schema`] &c).
    /// - Indexes.
    /// - Large blobs in the [`BlobStore`].
    ///
    /// Of these, the caller should inspect the blob store in order to account for memory usage by large blobs,
    /// and call [`Self::bytes_used_by_index_keys`] to account for indexes,
    /// but we intend to eat all the other overheads when billing.
    ///
    // TODO(perf, centril): consider storing the total number of granules in the table instead
    // so that this runs in constant time rather than O(|Pages|).
    pub fn bytes_used_by_rows(&self) -> u64 {
        self.pages()
            .iter()
            .map(|page| page.bytes_used_by_rows(self.inner.row_layout.size()) as u64)
            .sum()
    }

    #[cfg(test)]
    fn reconstruct_bytes_used_by_rows(&self) -> u64 {
        self.pages()
            .iter()
            .map(|page| unsafe {
                // Safety: `page` is in `self`, and was constructed using `self.innser.row_layout` and `self.inner.visitor_prog`,
                // so the three are mutually consistent.
                page.reconstruct_bytes_used_by_rows(self.inner.row_layout.size(), &self.inner.visitor_prog)
            } as u64)
            .sum()
    }

    /// Returns the number of indices in this table.
    pub fn num_indices(&self) -> usize {
        self.indexes.len()
    }

    /// Returns the number of rows (or [`RowPointer`]s, more accurately)
    /// stored in indexes by this table.
    ///
    /// This method runs in constant time.
    pub fn num_rows_in_indexes(&self) -> u64 {
        // Assume that each index contains all rows in the table.
        self.num_rows() * self.indexes.len() as u64
    }

    /// Returns the number of bytes used by keys stored in indexes by this table.
    ///
    /// This method scales in runtime with the number of indexes in the table,
    /// but not with the number of pages or rows.
    ///
    /// Key size is measured using a metric called "key size" or "data size,"
    /// which is intended to capture the number of live user-supplied bytes,
    /// not including representational overhead.
    /// This is distinct from the BFLATN size measured by [`Self::bytes_used_by_rows`].
    /// See the trait [`crate::btree_index::KeySize`] for specifics on the metric measured.
    pub fn bytes_used_by_index_keys(&self) -> u64 {
        self.indexes.values().map(|idx| idx.num_key_bytes()).sum()
    }
}

/// A reference to a single row within a table.
///
/// # Safety
///
/// Having a `r: RowRef` is a proof that [`r.pointer()`](RowRef::pointer) refers to a valid row.
/// This makes constructing a `RowRef`, i.e., `RowRef::new`, an `unsafe` operation.
#[derive(Copy, Clone)]
pub struct RowRef<'a> {
    /// The table that has the row at `self.pointer`.
    table: &'a TableInner,
    /// The blob store used in case there are blob hashes to resolve.
    blob_store: &'a dyn BlobStore,
    /// The pointer to the row in `self.table`.
    pointer: RowPointer,
}

impl fmt::Debug for RowRef<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("RowRef")
            .field("pointer", &self.pointer)
            .field("value", &self.to_product_value())
            .finish_non_exhaustive()
    }
}

impl<'a> RowRef<'a> {
    /// Construct a `RowRef` to the row at `pointer` within `table`.
    ///
    /// # Safety
    ///
    /// `pointer` must refer to a row within `table`
    /// which was previously inserted and has not been deleted since.
    ///
    /// This means:
    /// - The `PageIndex` of `pointer` must be in-bounds for `table.pages`.
    /// - The `PageOffset` of `pointer` must be properly aligned for the row type of `table`,
    ///   and must refer to a valid, live row in that page.
    /// - The `SquashedOffset` of `pointer` must match `table.squashed_offset`.
    ///
    /// Showing that `pointer` was the result of a call to `table.insert`
    /// and has not been passed to `table.delete`
    /// is sufficient to demonstrate all of these properties.
    unsafe fn new(
        table: &'a TableInner,
        blob_store: &'a dyn BlobStore,
        _squashed_offset: SquashedOffset,
        pointer: RowPointer,
    ) -> Self {
        debug_assert!(table.is_row_present(_squashed_offset, pointer));
        Self {
            table,
            blob_store,
            pointer,
        }
    }

    /// Extract a `ProductValue` from the table.
    ///
    /// This is a potentially expensive operation,
    /// as it must walk the table's `ProductTypeLayout`
    /// and heap-allocate various substructures of the `ProductValue`.
    pub fn to_product_value(&self) -> ProductValue {
        let res = self
            .serialize(ValueSerializer)
            .unwrap_or_else(|x| match x {})
            .into_product();
        // SAFETY: the top layer of a row when serialized is always a product.
        unsafe { res.unwrap_unchecked() }
    }

    /// Check that the `idx`th column of the row type stored by `self` is compatible with `T`,
    /// and read the value of that column from `self`.
    #[inline]
    pub fn read_col<T: ReadColumn>(self, col: impl Into<ColId>) -> Result<T, TypeError> {
        T::read_column(self, col.into().idx())
    }

    /// Construct a projection of the row at `self` by extracting the `cols`.
    ///
    /// If `cols` contains zero or more than one column, the values of the projected columns are wrapped in a [`ProductValue`].
    /// If `cols` is a single column, the value of that column is returned without wrapping in a `ProductValue`.
    ///
    /// # Safety
    ///
    /// - `cols` must not specify any column which is out-of-bounds for the row `self´.
    pub unsafe fn project_unchecked(self, cols: &ColList) -> AlgebraicValue {
        let col_layouts = &self.row_layout().product().elements;

        if let Some(head) = cols.as_singleton() {
            let head = head.idx();
            // SAFETY: caller promised that `head` is in-bounds of `col_layouts`.
            let col_layout = unsafe { col_layouts.get_unchecked(head) };
            // SAFETY:
            // - `col_layout` was just derived from the row layout.
            // - `AlgebraicValue` is compatible with any  `col_layout`.
            // - `self` is a valid row and offsetting to `col_layout` is valid.
            return unsafe { AlgebraicValue::unchecked_read_column(self, col_layout) };
        }
        let mut elements = Vec::with_capacity(cols.len() as usize);
        for col in cols.iter() {
            let col = col.idx();
            // SAFETY: caller promised that any `col` is in-bounds of `col_layouts`.
            let col_layout = unsafe { col_layouts.get_unchecked(col) };
            // SAFETY:
            // - `col_layout` was just derived from the row layout.
            // - `AlgebraicValue` is compatible with any  `col_layout`.
            // - `self` is a valid row and offsetting to `col_layout` is valid.
            elements.push(unsafe { AlgebraicValue::unchecked_read_column(self, col_layout) });
        }
        AlgebraicValue::product(elements)
    }

    /// Construct a projection of the row at `self` by extracting the `cols`.
    ///
    /// Returns an error if `cols` specifies an index which is out-of-bounds for the row at `self`.
    ///
    /// If `cols` contains zero or more than one column, the values of the projected columns are wrapped in a [`ProductValue`].
    /// If `cols` is a single column, the value of that column is returned without wrapping in a `ProductValue`.
    pub fn project(self, cols: &ColList) -> Result<AlgebraicValue, InvalidFieldError> {
        if let Some(head) = cols.as_singleton() {
            return self.read_col(head).map_err(|_| head.into());
        }
        let mut elements = Vec::with_capacity(cols.len() as usize);
        for col in cols.iter() {
            let col_val = self.read_col(col).map_err(|err| match err {
                TypeError::WrongType { .. } => {
                    unreachable!("AlgebraicValue::read_column never returns a `TypeError::WrongType`")
                }
                TypeError::IndexOutOfBounds { .. } => col,
            })?;
            elements.push(col_val);
        }
        Ok(AlgebraicValue::product(elements))
    }

    /// Returns the raw row pointer for this row reference.
    pub fn pointer(&self) -> RowPointer {
        self.pointer
    }

    /// Returns the blob store that any [`crate::blob_store::BlobHash`]es within the row refer to.
    pub(crate) fn blob_store(&self) -> &dyn BlobStore {
        self.blob_store
    }

    /// Return the layout of the row.
    ///
    /// All rows within the same table will have the same layout.
    pub fn row_layout(&self) -> &RowTypeLayout {
        &self.table.row_layout
    }

    /// Returns the page the row is in and the offset of the row within that page.
    pub fn page_and_offset(&self) -> (&Page, PageOffset) {
        self.table.page_and_offset(self.pointer())
    }

    /// Returns the bytes for the fixed portion of this row.
    pub(crate) fn get_row_data(&self) -> &Bytes {
        let (page, offset) = self.page_and_offset();
        page.get_row_data(offset, self.table.row_layout.size())
    }

    /// Returns the row hash for `ptr`.
    pub fn row_hash(&self) -> RowHash {
        RowHash(RowHash::hasher_builder().hash_one(self))
    }

    /// Returns the static layout for this row reference, if any.
    pub fn static_layout(&self) -> Option<&StaticLayout> {
        self.table.static_layout.as_ref().map(|(s, _)| s)
    }

    /// Encode the row referred to by `self` into a `Vec<u8>` using BSATN and then deserialize it.
    pub fn read_via_bsatn<T>(&self, scratch: &mut Vec<u8>) -> Result<T, ReadViaBsatnError>
    where
        T: DeserializeOwned,
    {
        self.to_bsatn_extend(scratch)?;
        Ok(bsatn::from_slice::<T>(scratch)?)
    }

    /// Return the number of bytes in the blob store to which this object holds a reference.
    ///
    /// Used to compute the table's `blob_store_bytes` when reconstructing a snapshot.
    ///
    /// Even within a single row, this is a conservative overestimate,
    /// as a row may contain multiple references to the same large blob.
    /// This seems unlikely to occur in practice.
    fn blob_store_bytes(&self) -> usize {
        let row_data = self.get_row_data();
        let (page, _) = self.page_and_offset();
        // SAFETY:
        // - Existence of a `RowRef` treated as proof
        //   of the row's validity and type information's correctness.
        unsafe { self.table.visitor_prog.visit_var_len(row_data) }
            .filter(|vlr| vlr.is_large_blob())
            .map(|vlr| {
                // SAFETY:
                // - Because `vlr.is_large_blob`, it points to exactly one granule.
                let granule = unsafe { page.iter_var_len_object(vlr.first_granule) }.next().unwrap();
                let blob_hash = granule.blob_hash();
                let blob = self.blob_store.retrieve_blob(&blob_hash).unwrap();

                blob.len()
            })
            .sum()
    }
}

impl Serialize for RowRef<'_> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let table = self.table;
        let (page, offset) = table.page_and_offset(self.pointer);
        // SAFETY: `ptr` points to a valid row in this table per above check.
        unsafe { serialize_row_from_page(ser, page, self.blob_store, offset, &table.row_layout) }
    }
}

impl ToBsatn for RowRef<'_> {
    /// BSATN-encode the row referred to by `self` into a freshly-allocated `Vec<u8>`.
    ///
    /// This method will use a [`StaticLayout`] if one is available,
    /// and may therefore be faster than calling [`bsatn::to_vec`].
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        if let Some(static_layout) = self.static_layout() {
            // Use fast path, by first fetching the row data and then using the static layout.
            let row = self.get_row_data();
            // SAFETY:
            // - Existence of a `RowRef` treated as proof
            //   of row's validity and type information's correctness.
            Ok(unsafe { static_layout.serialize_row_into_vec(row) })
        } else {
            bsatn::to_vec(self)
        }
    }

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf`, similar to [`Vec::extend`].
    ///
    /// This method will use a [`StaticLayout`] if one is available,
    /// and may therefore be faster than calling [`bsatn::to_writer`].
    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        if let Some(static_layout) = self.static_layout() {
            // Use fast path, by first fetching the row data and then using the static layout.
            let row = self.get_row_data();
            // SAFETY:
            // - Existence of a `RowRef` treated as proof
            //   of row's validity and type information's correctness.
            unsafe {
                static_layout.serialize_row_extend(buf, row);
            }
            Ok(())
        } else {
            // Use the slower, but more general, `bsatn_from` serializer to write the row.
            bsatn::to_writer(buf, self)
        }
    }

    fn static_bsatn_size(&self) -> Option<u16> {
        self.static_layout().map(|sl| sl.bsatn_length)
    }
}

impl Eq for RowRef<'_> {}
impl PartialEq for RowRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        // Ensure that the layouts are the same
        // so that we can use `eq_row_in_page`.
        // To do this, we first try address equality on the layouts.
        // This should succeed when the rows originate from the same table.
        // Otherwise, actually compare the layouts, which is expensive, but unlikely to happen.
        let a_ty = self.row_layout();
        let b_ty = other.row_layout();
        if !(ptr::eq(a_ty, b_ty) || a_ty == b_ty) {
            return false;
        }
        let (page_a, offset_a) = self.page_and_offset();
        let (page_b, offset_b) = other.page_and_offset();
        let static_layout = self.static_layout();
        // SAFETY: `offset_a/b` are valid rows in `page_a/b` typed at `a_ty`
        // and `static_bsatn_layout` is derived from `a_ty`.
        unsafe { eq_row_in_page(page_a, page_b, offset_a, offset_b, a_ty, static_layout) }
    }
}

impl PartialEq<ProductValue> for RowRef<'_> {
    fn eq(&self, rhs: &ProductValue) -> bool {
        let ty = self.row_layout();
        let (page, offset) = self.page_and_offset();
        // SAFETY: By having `RowRef`,
        // we know that `offset` is a valid offset for a row in `page` typed at `ty`.
        unsafe { eq_row_in_page_to_pv(self.blob_store, page, offset, rhs, ty) }
    }
}

impl Hash for RowRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (page, offset) = self.table.page_and_offset(self.pointer);
        let ty = &self.table.row_layout;
        // SAFETY: A `RowRef` is a proof that `self.pointer` refers to a live fixed row in `self.table`, so:
        // 1. `offset` points at a row in `page` lasting `ty.size()` bytes.
        // 2. the row is valid for `ty`.
        // 3. for any `vlr: VarLenRef` stored in the row,
        //    `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
        unsafe { hash_row_in_page(state, page, self.blob_store, offset, ty) };
    }
}

/// An iterator over all the rows, yielded as [`RowRef`]s, in a table.
pub struct TableScanIter<'table> {
    /// The current page we're yielding rows from.
    /// When `None`, the iterator will attempt to advance to the next page, if any.
    current_page: Option<FixedLenRowsIter<'table>>,
    /// The current page index we are or will visit.
    current_page_idx: PageIndex,
    /// The table the iterator is yielding rows from.
    pub(crate) table: &'table Table,
    /// The `BlobStore` that row references may refer into.
    pub(crate) blob_store: &'table dyn BlobStore,
}

impl<'a> Iterator for TableScanIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // This could have been written using `.flat_map`,
        // but we don't have `type Foo = impl Iterator<...>;` on stable yet.
        loop {
            match &mut self.current_page {
                // We're currently visiting a page,
                Some(iter_fixed_len) => {
                    if let Some(page_offset) = iter_fixed_len.next() {
                        // There's still at least one row in that page to visit,
                        // return a ref to that row.
                        let ptr =
                            RowPointer::new(false, self.current_page_idx, page_offset, self.table.squashed_offset);

                        // SAFETY: `offset` came from the `iter_fixed_len`, so it must point to a valid row.
                        let row_ref = unsafe { self.table.get_row_ref_unchecked(self.blob_store, ptr) };
                        return Some(row_ref);
                    } else {
                        // We've finished visiting that page, so set `current_page` to `None`,
                        // increment `self.current_page_idx` to the index of the next page,
                        // and go to the `None` case (1) in the match.
                        self.current_page = None;
                        self.current_page_idx.0 += 1;
                    }
                }

                // (1) If we aren't currently visiting a page,
                // the `else` case in the `Some` match arm
                // already incremented `self.current_page_idx`,
                // or we're just beginning and so it was initialized as 0.
                None => {
                    // If there's another page, set `self.current_page` to it,
                    // and go to the `Some` case in the match.
                    let next_page = self.table.pages().get(self.current_page_idx.idx())?;
                    let iter = next_page.iter_fixed_len(self.table.row_size());
                    self.current_page = Some(iter);
                }
            }
        }
    }
}

/// A combined table and index,
/// allowing direct extraction of a [`IndexScanIter`].
#[derive(Copy, Clone)]
pub struct TableAndIndex<'a> {
    table: &'a Table,
    blob_store: &'a dyn BlobStore,
    index: &'a TableIndex,
}

impl<'a> TableAndIndex<'a> {
    pub fn table(&self) -> &'a Table {
        self.table
    }

    pub fn index(&self) -> &'a TableIndex {
        self.index
    }

    /// Returns an iterator yielding all rows in this index for `key`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn seek_point(&self, key: &AlgebraicValue) -> IndexScanPointIter<'a> {
        IndexScanPointIter {
            table: self.table,
            blob_store: self.blob_store,
            btree_index_iter: self.index.seek_point(key),
        }
    }

    /// Returns an iterator yielding all rows in this index that fall within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn seek_range(&self, range: &impl RangeBounds<AlgebraicValue>) -> IndexScanRangeIter<'a> {
        IndexScanRangeIter {
            table: self.table,
            blob_store: self.blob_store,
            btree_index_iter: self.index.seek_range(range),
        }
    }
}

/// An iterator using a [`TableIndex`] to scan a `table`
/// for all the [`RowRef`]s matching the specified `key` in the indexed column(s).
///
/// Matching is defined by `Ord for AlgebraicValue`.
pub struct IndexScanPointIter<'a> {
    /// The table being scanned for rows.
    table: &'a Table,
    /// The blob store; passed on to the [`RowRef`]s in case they need it.
    blob_store: &'a dyn BlobStore,
    /// The iterator performing the index scan yielding row pointers.
    btree_index_iter: TableIndexPointIter<'a>,
}

impl<'a> Iterator for IndexScanPointIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.btree_index_iter.next().map(|ptr| {
            // SAFETY: `ptr` came from the index, which always holds pointers to valid rows for its table.
            unsafe { self.table.get_row_ref_unchecked(self.blob_store, ptr) }
        })
    }
}

/// An iterator using a [`TableIndex`] to scan a `table`
/// for all the [`RowRef`]s matching the specified `range` in the indexed column(s).
///
/// Matching is defined by `Ord for AlgebraicValue`.
pub struct IndexScanRangeIter<'a> {
    /// The table being scanned for rows.
    table: &'a Table,
    /// The blob store; passed on to the [`RowRef`]s in case they need it.
    blob_store: &'a dyn BlobStore,
    /// The iterator performing the index scan yielding row pointers.
    btree_index_iter: TableIndexRangeIter<'a>,
}

impl<'a> Iterator for IndexScanRangeIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.btree_index_iter.next().map(|ptr| {
            // SAFETY: `ptr` came from the index, which always holds pointers to valid rows for its table.
            unsafe { self.table.get_row_ref_unchecked(self.blob_store, ptr) }
        })
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
#[error("Unique constraint violation '{}' in table '{}': column(s): '{:?}' value: {}", constraint_name, table_name, cols, value.to_satn())]
pub struct UniqueConstraintViolation {
    pub constraint_name: Box<str>,
    pub table_name: Box<str>,
    pub cols: Vec<Box<str>>,
    pub value: AlgebraicValue,
}

impl UniqueConstraintViolation {
    /// Returns a unique constraint violation error for the given `index`
    /// and the `value` that would have been duplicated.
    #[cold]
    fn build(schema: &TableSchema, index: &TableIndex, index_id: IndexId, value: AlgebraicValue) -> Self {
        // Fetch the table name.
        let table_name = schema.table_name.clone();

        // Fetch the names of the columns used in the index.
        let cols = index
            .indexed_columns
            .iter()
            .map(|x| schema.columns()[x.idx()].col_name.clone())
            .collect();

        // Fetch the name of the index.
        let constraint_name = schema
            .indexes
            .iter()
            .find(|i| i.index_id == index_id)
            .unwrap()
            .index_name
            .clone();

        Self {
            constraint_name,
            table_name,
            cols,
            value,
        }
    }
}

// Private API:
impl Table {
    /// Returns a unique constraint violation error for the given `index`
    /// and the `value` that would have been duplicated.
    #[cold]
    pub fn build_error_unique(
        &self,
        index: &TableIndex,
        index_id: IndexId,
        value: AlgebraicValue,
    ) -> UniqueConstraintViolation {
        let schema = self.get_schema();
        UniqueConstraintViolation::build(schema, index, index_id, value)
    }

    /// Returns a new empty table using the particulars passed.
    fn new_raw(
        schema: Arc<TableSchema>,
        row_layout: RowTypeLayout,
        static_layout: Option<(StaticLayout, StaticBsatnValidator)>,
        visitor_prog: VarLenVisitorProgram,
        squashed_offset: SquashedOffset,
        pointer_map: Option<PointerMap>,
    ) -> Self {
        Self {
            inner: TableInner {
                row_layout,
                static_layout,
                visitor_prog,
                pages: Pages::default(),
            },
            is_scheduler: schema.schedule.is_some(),
            schema,
            indexes: BTreeMap::new(),
            pointer_map,
            squashed_offset,
            row_count: 0,
            blob_store_bytes: BlobNumBytes::default(),
        }
    }

    /// Returns whether the row at `ptr` is present or not.
    // TODO: Remove all uses of this method,
    //       or more likely, gate them behind `debug_assert!`
    //       so they don't have semantic meaning.
    //
    //       Unlike the previous `locking_tx_datastore::Table`'s `RowId`,
    //       `RowPointer` is not content-addressed.
    //       This means it is possible to:
    //       - have a `RowPointer` A* to row A,
    //       - Delete row A,
    //       - Insert row B into the same storage as freed from A,
    //       - Test `is_row_present(A*)`, which falsely reports that row A is still present.
    //
    //       In the final interface, this method is superfluous anyways,
    //       as `RowPointer` is not part of our public interface.
    //       Instead, we will always discover a known-present `RowPointer`
    //       during a table scan or index seek.
    //       As such, our `delete` and `insert` methods can be `unsafe`
    //       and trust that the `RowPointer` is valid.
    fn is_row_present(&self, ptr: RowPointer) -> bool {
        if self.squashed_offset != ptr.squashed_offset() {
            return false;
        }
        let Some((page, offset)) = self.inner.try_page_and_offset(ptr) else {
            return false;
        };
        page.has_row_offset(self.row_size(), offset)
    }

    /// Returns the row size for a row in the table.
    pub fn row_size(&self) -> Size {
        self.row_layout().size()
    }

    /// Returns the layout for a row in the table.
    fn row_layout(&self) -> &RowTypeLayout {
        &self.inner.row_layout
    }

    /// Returns the pages storing the physical rows of this table.
    fn pages(&self) -> &Pages {
        &self.inner.pages
    }

    /// Iterates over each [`Page`] in this table, ensuring that its hash is computed before yielding it.
    ///
    /// Used when capturing a snapshot.
    pub fn iter_pages_with_hashes(&mut self) -> impl Iterator<Item = (blake3::Hash, &Page)> {
        self.inner.pages.iter_mut().map(|page| {
            let hash = page.save_or_get_content_hash();
            (hash, &**page)
        })
    }

    /// Returns the number of pages storing the physical rows of this table.
    fn num_pages(&self) -> usize {
        self.inner.pages.len()
    }

    /// Returns the [`StaticLayout`] for this table,
    pub(crate) fn static_layout(&self) -> Option<&StaticLayout> {
        self.inner.static_layout.as_ref().map(|(s, _)| s)
    }

    /// Rebuild the [`PointerMap`] by iterating over all the rows in `self` and inserting them.
    ///
    /// Called when restoring from a snapshot after installing the pages,
    /// but after computing the row count,
    /// since snapshots do not save the pointer map..
    fn rebuild_pointer_map(&mut self, blob_store: &dyn BlobStore) -> PointerMap {
        // TODO(perf): Pre-allocate `PointerMap.map` with capacity `self.row_count`.
        // Alternatively, do this at the same time as `compute_row_count`.
        self.scan_rows(blob_store)
            .map(|row_ref| (row_ref.row_hash(), row_ref.pointer()))
            .collect()
    }

    /// Compute and store `self.row_count` and `self.blob_store_bytes`
    /// by iterating over all the rows in `self` and counting them.
    ///
    /// Called when restoring from a snapshot after installing the pages,
    /// since snapshots do not save this metadata.
    fn compute_row_count(&mut self, blob_store: &dyn BlobStore) {
        let mut row_count = 0;
        let mut blob_store_bytes = 0;
        for row in self.scan_rows(blob_store) {
            row_count += 1;
            blob_store_bytes += row.blob_store_bytes();
        }
        self.row_count = row_count as u64;
        self.blob_store_bytes = blob_store_bytes.into();
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::blob_store::{HashMapBlobStore, NullBlobStore};
    use crate::page::tests::hash_unmodified_save_get;
    use crate::var_len::VarLenGranule;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseResult;
    use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, RawModuleDefV9Builder};
    use spacetimedb_primitives::{col_list, TableId};
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::proptest::{generate_typed_row, generate_typed_row_vec};
    use spacetimedb_sats::{product, AlgebraicType, ArrayValue};
    use spacetimedb_schema::def::{BTreeAlgorithm, ModuleDef};
    use spacetimedb_schema::schema::Schema as _;

    /// Create a `Table` from a `ProductType` without validation.
    pub(crate) fn table(ty: ProductType) -> Table {
        // Use a fast path here to avoid slowing down Miri in the proptests.
        // Does not perform validation.
        let schema = TableSchema::from_product_type(ty);
        Table::new(schema.into(), SquashedOffset::COMMITTED_STATE)
    }

    #[test]
    fn unique_violation_error() {
        let table_name = "UniqueIndexed";
        let index_name = "UniqueIndexed_unique_col_idx_btree";
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                table_name,
                ProductType::from([("unique_col", AlgebraicType::I32), ("other_col", AlgebraicType::I32)]),
                true,
            )
            .with_unique_constraint(0)
            .with_index(
                RawIndexAlgorithm::BTree { columns: col_list![0] },
                "accessor_name_doesnt_matter",
            );

        let def: ModuleDef = builder.finish().try_into().expect("Failed to build schema");

        let schema = TableSchema::from_module_def(&def, def.table(table_name).unwrap(), (), TableId::SENTINEL);
        assert_eq!(schema.indexes.len(), 1);
        let index_schema = schema.indexes[0].clone();

        let mut table = Table::new(schema.into(), SquashedOffset::COMMITTED_STATE);
        let pool = PagePool::new_for_test();
        let cols = ColList::new(0.into());
        let algo = BTreeAlgorithm { columns: cols.clone() }.into();

        let index = table.new_index(&algo, true).unwrap();
        // SAFETY: Index was derived from `table`.
        unsafe { table.insert_index(&NullBlobStore, index_schema.index_id, index) };

        // Reserve a page so that we can check the hash.
        let pi = table.inner.pages.reserve_empty_page(&pool, table.row_size()).unwrap();
        let hash_pre_ins = hash_unmodified_save_get(&mut table.inner.pages[pi]);

        // Insert the row (0, 0).
        table
            .insert(&pool, &mut NullBlobStore, &product![0i32, 0i32])
            .expect("Initial insert failed");

        // Inserting cleared the hash.
        let hash_post_ins = hash_unmodified_save_get(&mut table.inner.pages[pi]);
        assert_ne!(hash_pre_ins, hash_post_ins);

        // Try to insert the row (0, 1), and assert that we get the expected error.
        match table.insert(&pool, &mut NullBlobStore, &product![0i32, 1i32]) {
            Ok(_) => panic!("Second insert with same unique value succeeded"),
            Err(InsertError::IndexError(UniqueConstraintViolation {
                constraint_name,
                table_name,
                cols,
                value,
            })) => {
                assert_eq!(&*constraint_name, index_name);
                assert_eq!(&*table_name, "UniqueIndexed");
                assert_eq!(cols.iter().map(|c| c.to_string()).collect::<Vec<_>>(), &["unique_col"]);
                assert_eq!(value, AlgebraicValue::I32(0));
            }
            Err(e) => panic!("Expected UniqueConstraintViolation but found {:?}", e),
        }

        // Second insert did clear the hash while we had a constraint violation,
        // as constraint checking is done after insertion and then rolled back.
        assert_eq!(table.inner.pages[pi].unmodified_hash(), None);
    }

    fn insert_retrieve_body(ty: impl Into<ProductType>, val: impl Into<ProductValue>) -> TestCaseResult {
        let val = val.into();
        let pool = PagePool::new_for_test();
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(ty.into());
        let (hash, row) = table.insert(&pool, &mut blob_store, &val).unwrap();
        let hash = hash.unwrap();
        prop_assert_eq!(row.row_hash(), hash);
        let ptr = row.pointer();
        prop_assert_eq!(table.pointers_for(hash), &[ptr]);

        prop_assert_eq!(table.inner.pages.len(), 1);
        prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);

        let row_ref = table.get_row_ref(&blob_store, ptr).unwrap();
        prop_assert_eq!(row_ref.to_product_value(), val.clone());
        let bsatn_val = to_vec(&val).unwrap();
        prop_assert_eq!(&bsatn_val, &to_vec(&row_ref).unwrap());
        prop_assert_eq!(&bsatn_val, &row_ref.to_bsatn_vec().unwrap());

        prop_assert_eq!(
            &table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(),
            &[ptr]
        );

        Ok(())
    }

    #[test]
    fn repro_serialize_bsatn_empty_array() {
        let ty = AlgebraicType::array(AlgebraicType::U64);
        let arr = ArrayValue::from(Vec::<u64>::new().into_boxed_slice());
        insert_retrieve_body(ty, AlgebraicValue::from(arr)).unwrap();
    }

    #[test]
    fn repro_serialize_bsatn_debug_assert() {
        let ty = AlgebraicType::array(AlgebraicType::U64);
        let arr = ArrayValue::from((0..130u64).collect::<Box<_>>());
        insert_retrieve_body(ty, AlgebraicValue::from(arr)).unwrap();
    }

    fn reconstruct_index_num_key_bytes(table: &Table, blob_store: &dyn BlobStore, index_id: IndexId) -> u64 {
        let index = table.get_index_by_id(index_id).unwrap();

        index
            .seek_range(&(..))
            .map(|row_ptr| {
                let row_ref = table.get_row_ref(blob_store, row_ptr).unwrap();
                let key = row_ref.project(&index.indexed_columns).unwrap();
                crate::table_index::KeySize::key_size_in_bytes(&key) as u64
            })
            .sum()
    }

    /// Given a row type `ty`, a set of rows of that type `vals`,
    /// and a set of columns within that type `indexed_columns`,
    /// populate a table with `vals`, add an index on the `indexed_columns`,
    /// and perform various assertions that the reported index size metrics are correct.
    fn test_index_size_reporting(
        ty: ProductType,
        vals: Vec<ProductValue>,
        indexed_columns: ColList,
    ) -> Result<(), TestCaseError> {
        let pool = PagePool::new_for_test();
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(ty.clone());

        for row in &vals {
            prop_assume!(table.insert(&pool, &mut blob_store, row).is_ok());
        }

        // We haven't added any indexes yet, so there should be 0 rows in indexes.
        prop_assert_eq!(table.num_rows_in_indexes(), 0);

        let index_id = IndexId(0);

        let algo = BTreeAlgorithm {
            columns: indexed_columns.clone(),
        }
        .into();
        let index = TableIndex::new(&ty, &algo, false).unwrap();
        // Add an index on column 0.
        // Safety:
        // We're using `ty` as the row type for both `table` and the new index.
        unsafe { table.insert_index(&blob_store, index_id, index) };

        // We have one index, which should be fully populated,
        // so in total we should have the same number of rows in indexes as we have rows.
        prop_assert_eq!(table.num_rows_in_indexes(), table.num_rows());

        let index = table.get_index_by_id(index_id).unwrap();

        // One index, so table's reporting of bytes used should match that index's reporting.
        prop_assert_eq!(table.bytes_used_by_index_keys(), index.num_key_bytes());

        // Walk all the rows in the index, sum their key size,
        // and assert it matches the `index.num_key_bytes()`
        prop_assert_eq!(
            index.num_key_bytes(),
            reconstruct_index_num_key_bytes(&table, &blob_store, index_id)
        );

        // Walk all the rows we inserted, project them to the cols that will be their keys,
        // sum their key size,
        // and assert it matches the `index.num_key_bytes()`
        let key_size_in_pvs = vals
            .iter()
            .map(|row| crate::table_index::KeySize::key_size_in_bytes(&row.project(&indexed_columns).unwrap()) as u64)
            .sum();
        prop_assert_eq!(index.num_key_bytes(), key_size_in_pvs);

        let algo = BTreeAlgorithm {
            columns: indexed_columns,
        }
        .into();
        let index = TableIndex::new(&ty, &algo, false).unwrap();
        // Add a duplicate of the same index, so we can check that all above quantities double.
        // Safety:
        // As above, we're using `ty` as the row type for both `table` and the new index.
        unsafe { table.insert_index(&blob_store, IndexId(1), index) };

        prop_assert_eq!(table.num_rows_in_indexes(), table.num_rows() * 2);
        prop_assert_eq!(table.bytes_used_by_index_keys(), key_size_in_pvs * 2);

        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x10000000, ..Default::default() })]

        #[test]
        fn insert_retrieve((ty, val) in generate_typed_row()) {
            insert_retrieve_body(ty, val)?;
        }

        #[test]
        fn insert_delete_removed_from_pointer_map((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);
            let (hash, row) = table.insert(&pool, &mut blob_store, &val).unwrap();
            let hash = hash.unwrap();
            prop_assert_eq!(row.row_hash(), hash);
            let ptr = row.pointer();
            prop_assert_eq!(table.pointers_for(hash), &[ptr]);

            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);
            prop_assert_eq!(table.row_count, 1);

            let hash_pre_del = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);

            table.delete(&mut blob_store, ptr, |_| ());

            let hash_post_del = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);
            assert_ne!(hash_pre_del, hash_post_del);

            prop_assert_eq!(table.pointers_for(hash), &[]);

            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 0);
            prop_assert_eq!(table.row_count, 0);

            prop_assert!(&table.scan_rows(&blob_store).next().is_none());
        }

        #[test]
        fn insert_duplicate_set_semantic((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);

            let (hash, row) = table.insert(&pool, &mut blob_store, &val).unwrap();
            let hash = hash.unwrap();
            prop_assert_eq!(row.row_hash(), hash);
            let ptr = row.pointer();
            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.pointers_for(hash), &[ptr]);
            prop_assert_eq!(table.row_count, 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            let blob_uses = blob_store.usage_counter();

            let hash_pre_ins = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);

            prop_assert!(table.insert(&pool, &mut blob_store, &val).is_err());

            // Hash was cleared and is different despite failure to insert.
            let hash_post_ins = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);
            assert_ne!(hash_pre_ins, hash_post_ins);

            prop_assert_eq!(table.row_count, 1);
            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.pointers_for(hash), &[ptr]);

            let blob_uses_after = blob_store.usage_counter();

            prop_assert_eq!(blob_uses_after, blob_uses);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);
        }

        #[test]
        fn insert_bsatn_same_as_pv((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut bs_pv = HashMapBlobStore::default();
            let mut table_pv = table(ty.clone());
            let res_pv = table_pv.insert(&pool, &mut bs_pv, &val);

            let mut bs_bsatn = HashMapBlobStore::default();
            let mut table_bsatn = table(ty);
            let res_bsatn = insert_bsatn(&mut table_bsatn, &mut bs_bsatn, &val);

            prop_assert_eq!(res_pv, res_bsatn);
            prop_assert_eq!(bs_pv, bs_bsatn);
            prop_assert_eq!(table_pv, table_bsatn);
        }

        #[test]
        fn row_size_reporting_matches_slow_implementations((ty, vals) in generate_typed_row_vec(128, 2048)) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty.clone());

            for row in &vals {
                prop_assume!(table.insert(&pool, &mut blob_store, row).is_ok());
            }

            prop_assert_eq!(table.bytes_used_by_rows(), table.reconstruct_bytes_used_by_rows());
            prop_assert_eq!(table.num_rows(), table.reconstruct_num_rows());
            prop_assert_eq!(table.num_rows(), vals.len() as u64);

            // TODO(testing): Determine if there's a meaningful way to test that the blob store reporting is correct.
            // I (pgoldman 2025-01-27) doubt it, as the test would be "visit every blob and sum their size,"
            // which is already what the actual implementation does.
        }

        #[test]
        fn index_size_reporting_matches_slow_implementations_single_column((ty, vals) in generate_typed_row_vec(128, 2048)) {
            prop_assume!(!ty.elements.is_empty());

            test_index_size_reporting(ty, vals, ColList::from(ColId(0)))?;
        }

        #[test]
        fn index_size_reporting_matches_slow_implementations_two_column((ty, vals) in generate_typed_row_vec(128, 2048)) {
            prop_assume!(ty.elements.len() >= 2);


            test_index_size_reporting(ty, vals, ColList::from([ColId(0), ColId(1)]))?;
        }
    }

    fn insert_bsatn<'a>(
        table: &'a mut Table,
        blob_store: &'a mut dyn BlobStore,
        val: &ProductValue,
    ) -> Result<(Option<RowHash>, RowRef<'a>), InsertError> {
        let row = &to_vec(&val).unwrap();

        // Optimistically insert the `row` before checking any constraints
        // under the assumption that errors (unique constraint & set semantic violations) are rare.
        let pool = PagePool::new_for_test();
        let (row_ref, blob_bytes) = table.insert_physically_bsatn(&pool, blob_store, row)?;
        let row_ptr = row_ref.pointer();

        // Confirm the insertion, checking any constraints, removing the physical row on error.
        // SAFETY: We just inserted `ptr`, so it must be present.
        let (hash, row_ptr) = unsafe { table.confirm_insertion::<true>(blob_store, row_ptr, blob_bytes) }?;
        // SAFETY: Per post-condition of `confirm_insertion`, `row_ptr` refers to a valid row.
        let row_ref = unsafe { table.get_row_ref_unchecked(blob_store, row_ptr) };
        Ok((hash, row_ref))
    }

    // Compare `scan_rows` against a simpler implementation.
    #[test]
    fn table_scan_iter_eq_flatmap() {
        let pool = PagePool::new_for_test();
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(AlgebraicType::U64.into());
        for v in 0..2u64.pow(14) {
            table.insert(&pool, &mut blob_store, &product![v]).unwrap();
        }

        let complex = table.scan_rows(&blob_store).map(|r| r.pointer());
        let simple = table
            .inner
            .pages
            .iter()
            .zip((0..).map(PageIndex))
            .flat_map(|(page, pi)| {
                page.iter_fixed_len(table.row_size())
                    .map(move |po| RowPointer::new(false, pi, po, table.squashed_offset))
            });
        assert!(complex.eq(simple));
    }

    #[test]
    #[should_panic]
    fn read_row_unaligned_page_offset_soundness() {
        // Insert a `u64` into a table.
        let pt = AlgebraicType::U64.into();
        let pv = product![42u64];
        let mut table = table(pt);
        let pool = &PagePool::new_for_test();
        let blob_store = &mut NullBlobStore;
        let (_, row_ref) = table.insert(pool, blob_store, &pv).unwrap();

        // Manipulate the page offset to 1 instead of 0.
        // This now points into the "middle" of a row.
        let ptr = row_ref.pointer().with_page_offset(PageOffset(1));

        // We expect this to panic.
        // Miri should not have any issue with this call either.
        table.get_row_ref(&NullBlobStore, ptr).unwrap().to_product_value();
    }

    #[test]
    fn test_blob_store_bytes() {
        let pt: ProductType = [AlgebraicType::String, AlgebraicType::I32].into();
        let pool = &PagePool::new_for_test();
        let blob_store = &mut HashMapBlobStore::default();
        let mut insert = |table: &mut Table, string, num| {
            table
                .insert(pool, blob_store, &product![string, num])
                .unwrap()
                .1
                .pointer()
        };
        let mut table1 = table(pt.clone());

        // Insert short string, `blob_store_bytes` should be 0.
        let short_str = std::str::from_utf8(&[98; 6]).unwrap();
        let short_row_ptr = insert(&mut table1, short_str, 0);
        assert_eq!(table1.blob_store_bytes.0, 0);

        // Insert long string, `blob_store_bytes` should be the length of the string.
        const BLOB_OBJ_LEN: BlobNumBytes = BlobNumBytes(VarLenGranule::OBJECT_SIZE_BLOB_THRESHOLD + 1);
        let long_str = std::str::from_utf8(&[98; BLOB_OBJ_LEN.0]).unwrap();
        let long_row_ptr = insert(&mut table1, long_str, 0);
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN);

        // Insert previous long string in the same table,
        // `blob_store_bytes` should count the length twice,
        // even though `HashMapBlobStore` deduplicates it.
        let long_row_ptr2 = insert(&mut table1, long_str, 1);
        const BLOB_OBJ_LEN_2X: BlobNumBytes = BlobNumBytes(BLOB_OBJ_LEN.0 * 2);
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN_2X);

        // Insert previous long string in a new table,
        // `blob_store_bytes` should show the length,
        // even though `HashMapBlobStore` deduplicates it.
        let mut table2 = table(pt);
        let _ = insert(&mut table2, long_str, 0);
        assert_eq!(table2.blob_store_bytes, BLOB_OBJ_LEN);

        // Delete `short_str` row. This should not affect the byte count.
        table1.delete(blob_store, short_row_ptr, |_| ()).unwrap();
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN_2X);

        // Delete the first long string row. This gets us down to `BLOB_OBJ_LEN` (we had 2x before).
        table1.delete(blob_store, long_row_ptr, |_| ()).unwrap();
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN);

        // Delete the first long string row. This gets us down to 0 (we've now deleted 2x).
        table1.delete(blob_store, long_row_ptr2, |_| ()).unwrap();
        assert_eq!(table1.blob_store_bytes, 0.into());
    }

    /// Assert that calling `get_row_ref` to get a row ref to a non-existent `RowPointer`
    /// does not panic.
    #[test]
    fn get_row_ref_no_panic() {
        let blob_store = &mut HashMapBlobStore::default();
        let table = table([AlgebraicType::String, AlgebraicType::I32].into());

        // This row pointer has an incorrect `SquashedOffset`, and so does not point into `table`.
        assert!(table
            .get_row_ref(
                blob_store,
                RowPointer::new(false, PageIndex(0), PageOffset(0), SquashedOffset::TX_STATE),
            )
            .is_none());

        // This row pointer has the correct `SquashedOffset`, but points out-of-bounds within `table`.
        assert!(table
            .get_row_ref(
                blob_store,
                RowPointer::new(false, PageIndex(0), PageOffset(0), SquashedOffset::COMMITTED_STATE),
            )
            .is_none());
    }
}
