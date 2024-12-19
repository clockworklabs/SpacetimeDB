use std::ops::{Deref, RangeBounds};

use anyhow::{anyhow, Result};
use iter::Iter;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_physical_plan::plan::{ProjectField, ProjectPlan, TupleField};
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_table::{
    blob_store::BlobStore,
    static_assert_size,
    table::{IndexScanIter, RowRef, Table, TableScanIter},
};

pub mod iter;

pub trait Datastore {
    fn table(&self, table_id: TableId) -> Option<&Table>;
    fn blob_store(&self) -> &dyn BlobStore;
}

pub struct FallibleDatastore<'a, T>(&'a T);

impl<T> Deref for FallibleDatastore<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, T: Datastore> FallibleDatastore<'a, T> {
    pub fn table(&self, table_id: TableId) -> Result<&Table> {
        self.0
            .table(table_id)
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
    }

    pub fn table_scan(&self, table_id: TableId) -> Result<TableScanIter> {
        self.0
            .table(table_id)
            .map(|table| table.scan_rows(self.0.blob_store()))
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
    }

    pub fn index_scan(
        &self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Result<IndexScanIter> {
        self.0
            .table(table_id)
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
            .and_then(|table| {
                table
                    .index_seek_by_id(self.0.blob_store(), index_id, range)
                    .ok_or_else(|| anyhow!("IndexId `{index_id}` does not exist"))
            })
    }
}

/// Each query operator returns a tuple of [RowRef]s
#[derive(Clone)]
pub enum Tuple<'a> {
    /// A pointer to a row in a base table
    Row(RowRef<'a>),
    /// A temporary returned by a join operator
    Join(Vec<RowRef<'a>>),
}

static_assert_size!(Tuple, 32);

impl ProjectField for Tuple<'_> {
    fn project(&self, field: &TupleField) -> AlgebraicValue {
        match self {
            Self::Row(ptr) => ptr.read_col(field.field_pos).unwrap(),
            Self::Join(ptrs) => field
                .label_pos
                .and_then(|i| ptrs.get(i))
                .map(|ptr| ptr.read_col(field.field_pos).unwrap())
                .unwrap(),
        }
    }
}

impl<'a> Tuple<'a> {
    /// Select the tuple element at position `i`
    pub fn select(self, i: usize) -> Option<RowRef<'a>> {
        match self {
            Self::Row(_) => None,
            Self::Join(mut ptrs) => Some(ptrs.swap_remove(i)),
        }
    }

    /// Append a [RowRef] to a tuple
    pub fn append(self, ptr: RowRef<'a>) -> Self {
        match self {
            Self::Row(row) => Self::Join(vec![row, ptr]),
            Self::Join(mut rows) => {
                rows.push(ptr);
                Self::Join(rows)
            }
        }
    }
}

/// Execute a physcial plan, pushing the return tuples into a closure.
pub fn execute_plan<T: Datastore>(
    plan: &ProjectPlan,
    tx: &FallibleDatastore<'_, T>,
    mut f: impl FnMut(RowRef),
) -> Result<()> {
    match plan {
        ProjectPlan::None(plan) => {
            for tuple in Iter::build(plan, tx)? {
                if let Tuple::Row(ptr) = tuple {
                    f(ptr);
                }
            }
        }
        ProjectPlan::Name(plan, _, Some(i)) => {
            for tuple in Iter::build(plan, tx)? {
                if let Some(ptr) = tuple.select(*i) {
                    f(ptr);
                }
            }
        }
        _ => {}
    }
    Ok(())
}
