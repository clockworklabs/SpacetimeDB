use std::ops::RangeBounds;

use anyhow::{anyhow, Result};
use iter::PlanIter;
use spacetimedb_lib::{
    bsatn::{EncodeError, ToBsatn},
    query::Delta,
    ser::Serialize,
    AlgebraicValue, ProductValue,
};
use spacetimedb_physical_plan::plan::{ProjectField, ProjectPlan, TupleField};
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_table::{
    blob_store::BlobStore,
    static_assert_size,
    table::{IndexScanIter, RowRef, Table, TableScanIter},
};

pub mod iter;

/// The datastore interface required for building an executor
pub trait Datastore {
    fn table(&self, table_id: TableId) -> Option<&Table>;
    fn blob_store(&self) -> &dyn BlobStore;

    fn table_or_err(&self, table_id: TableId) -> Result<&Table> {
        self.table(table_id)
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
    }

    fn table_scan(&self, table_id: TableId) -> Result<TableScanIter> {
        self.table(table_id)
            .map(|table| table.scan_rows(self.blob_store()))
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
    }

    fn index_scan(
        &self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Result<IndexScanIter> {
        self.table(table_id)
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
            .and_then(|table| {
                table
                    .index_seek_by_id(self.blob_store(), index_id, range)
                    .ok_or_else(|| anyhow!("IndexId `{index_id}` does not exist"))
            })
    }
}

pub trait DeltaStore {
    fn has_inserts(&self, table_id: TableId) -> Option<Delta>;
    fn has_deletes(&self, table_id: TableId) -> Option<Delta>;

    fn inserts_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>>;
    fn deletes_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>>;

    fn delta_scan(&self, table_id: TableId, inserts: bool) -> Result<DeltaScanIter> {
        match inserts {
            true => self
                .inserts_for_table(table_id)
                .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
                .map(|iter| DeltaScanIter { iter }),
            false => self
                .deletes_for_table(table_id)
                .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
                .map(|iter| DeltaScanIter { iter }),
        }
    }
}

#[derive(Clone, Serialize)]
pub enum Row<'a> {
    Ptr(RowRef<'a>),
    Ref(&'a ProductValue),
}

impl ToBsatn for Row<'_> {
    fn static_bsatn_size(&self) -> Option<u16> {
        match self {
            Self::Ptr(ptr) => ptr.static_bsatn_size(),
            Self::Ref(val) => val.static_bsatn_size(),
        }
    }

    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> std::result::Result<(), EncodeError> {
        match self {
            Self::Ptr(ptr) => ptr.to_bsatn_extend(buf),
            Self::Ref(val) => val.to_bsatn_extend(buf),
        }
    }

    fn to_bsatn_vec(&self) -> std::result::Result<Vec<u8>, EncodeError> {
        match self {
            Self::Ptr(ptr) => ptr.to_bsatn_vec(),
            Self::Ref(val) => val.to_bsatn_vec(),
        }
    }
}

impl ProjectField for Row<'_> {
    fn project(&self, field: &TupleField) -> AlgebraicValue {
        match self {
            Self::Ptr(ptr) => ptr.read_col(field.field_pos).unwrap(),
            Self::Ref(val) => val.elements.get(field.field_pos).unwrap().clone(),
        }
    }
}

/// Each query operator returns a tuple of [RowRef]s
#[derive(Clone)]
pub enum Tuple<'a> {
    /// A pointer to a row in a base table
    Row(Row<'a>),
    /// A temporary returned by a join operator
    Join(Vec<Row<'a>>),
}

static_assert_size!(Tuple, 40);

impl ProjectField for Tuple<'_> {
    fn project(&self, field: &TupleField) -> AlgebraicValue {
        match self {
            Self::Row(row) => row.project(field),
            Self::Join(ptrs) => field
                .label_pos
                .and_then(|i| ptrs.get(i))
                .map(|ptr| ptr.project(field))
                .unwrap(),
        }
    }
}

impl<'a> Tuple<'a> {
    /// Select the tuple element at position `i`
    fn select(self, i: usize) -> Option<Row<'a>> {
        match self {
            Self::Row(_) => None,
            Self::Join(mut ptrs) => Some(ptrs.swap_remove(i)),
        }
    }

    /// Append a [Row] to a tuple
    fn append(self, ptr: Row<'a>) -> Self {
        match self {
            Self::Row(row) => Self::Join(vec![row, ptr]),
            Self::Join(mut rows) => {
                rows.push(ptr);
                Self::Join(rows)
            }
        }
    }
}

pub struct DeltaScanIter<'a> {
    iter: std::slice::Iter<'a, ProductValue>,
}

impl<'a> Iterator for DeltaScanIter<'a> {
    type Item = &'a ProductValue;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// Execute a query plan.
/// The actual execution is driven by `f`.
pub fn execute_plan<T, R>(plan: &ProjectPlan, tx: &T, f: impl Fn(PlanIter) -> R) -> Result<R>
where
    T: Datastore + DeltaStore,
{
    PlanIter::build(plan, tx).map(f)
}
