use std::{
    hash::{Hash, Hasher},
    ops::RangeBounds,
};

use anyhow::{anyhow, Result};
use iter::PlanIter;
use spacetimedb_lib::{
    bsatn::{EncodeError, ToBsatn},
    query::Delta,
    sats::impl_serialize,
    AlgebraicValue, ProductValue,
};
use spacetimedb_physical_plan::plan::{ProjectField, ProjectPlan, TupleField};
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_table::{
    blob_store::BlobStore,
    static_assert_size,
    table::{IndexScanPointIter, IndexScanRangeIter, RowRef, Table, TableScanIter},
};

pub mod dml;
pub mod iter;
pub mod pipelined;

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

    fn index_scan_point(
        &self,
        table_id: TableId,
        index_id: IndexId,
        key: &AlgebraicValue,
    ) -> Result<IndexScanPointIter> {
        self.table(table_id)
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
            .and_then(|table| {
                table
                    .get_index_by_id_with_table(self.blob_store(), index_id)
                    .map(|i| i.seek_point(key))
                    .ok_or_else(|| anyhow!("IndexId `{index_id}` does not exist"))
            })
    }

    fn index_scan_range(
        &self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Result<IndexScanRangeIter> {
        self.table(table_id)
            .ok_or_else(|| anyhow!("TableId `{table_id}` does not exist"))
            .and_then(|table| {
                table
                    .get_index_by_id_with_table(self.blob_store(), index_id)
                    .map(|i| i.seek_range(range))
                    .ok_or_else(|| anyhow!("IndexId `{index_id}` does not exist"))
            })
    }
}

pub trait DeltaStore {
    fn num_inserts(&self, table_id: TableId) -> usize;
    fn num_deletes(&self, table_id: TableId) -> usize;

    fn has_inserts(&self, table_id: TableId) -> bool {
        self.num_inserts(table_id) != 0
    }

    fn has_deletes(&self, table_id: TableId) -> bool {
        self.num_deletes(table_id) != 0
    }

    fn inserts_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>>;
    fn deletes_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>>;

    fn index_scan_range_for_delta(
        &self,
        table_id: TableId,
        index_id: IndexId,
        delta: Delta,
        range: impl RangeBounds<AlgebraicValue>,
    ) -> impl Iterator<Item = Row>;

    fn index_scan_point_for_delta(
        &self,
        table_id: TableId,
        index_id: IndexId,
        delta: Delta,
        point: &AlgebraicValue,
    ) -> impl Iterator<Item = Row>;

    fn delta_scan(&self, table_id: TableId, inserts: bool) -> DeltaScanIter {
        match inserts {
            true => DeltaScanIter {
                iter: self.inserts_for_table(table_id),
            },
            false => DeltaScanIter {
                iter: self.deletes_for_table(table_id),
            },
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum Row<'a> {
    Ptr(RowRef<'a>),
    Ref(&'a ProductValue),
}

impl Hash for Row<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Ptr(x) => x.hash(state),
            Self::Ref(x) => x.hash(state),
        }
    }
}

impl Row<'_> {
    pub fn to_product_value(&self) -> ProductValue {
        match self {
            Self::Ptr(ptr) => ptr.to_product_value(),
            Self::Ref(val) => (*val).clone(),
        }
    }
}

impl_serialize!(['a] Row<'a>, (self, ser) => match self {
    Self::Ptr(row) => row.serialize(ser),
    Self::Ref(row) => row.serialize(ser),
});

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
            Self::Ptr(ptr) => ptr.project(field),
            Self::Ref(val) => val.project(field),
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

    fn join(self, with: Self) -> Self {
        match with {
            Self::Row(ptr) => self.append(ptr),
            Self::Join(ptrs) => ptrs.into_iter().fold(self, |tup, ptr| tup.append(ptr)),
        }
    }
}

pub struct DeltaScanIter<'a> {
    iter: Option<std::slice::Iter<'a, ProductValue>>,
}

impl<'a> Iterator for DeltaScanIter<'a> {
    type Item = &'a ProductValue;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.as_mut().and_then(|iter| iter.next())
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
