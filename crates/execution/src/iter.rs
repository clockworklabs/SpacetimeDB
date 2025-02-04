use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Result};
use spacetimedb_lib::{query::Delta, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::plan::{
    HashJoin, IxJoin, IxScan, PhysicalExpr, PhysicalPlan, ProjectField, ProjectPlan, Sarg, Semi, TupleField,
};
use spacetimedb_table::{
    blob_store::BlobStore,
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    table::{IndexScanIter, Table, TableScanIter},
};

use crate::{Datastore, DeltaScanIter, DeltaStore, Row, Tuple};

/// The different iterators for evaluating query plans
pub enum PlanIter<'a> {
    Table(TableScanIter<'a>),
    Index(IndexScanIter<'a>),
    Delta(DeltaScanIter<'a>),
    RowId(RowRefIter<'a>),
    Tuple(ProjectIter<'a>),
}

impl<'a> PlanIter<'a> {
    pub(crate) fn build<Tx>(plan: &'a ProjectPlan, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        ProjectIter::build(plan, tx).map(|iter| match iter {
            ProjectIter::None(Iter::Row(RowRefIter::TableScan(iter))) => Self::Table(iter),
            ProjectIter::None(Iter::Row(RowRefIter::IndexScan(iter))) => Self::Index(iter),
            ProjectIter::None(Iter::Row(iter)) => Self::RowId(iter),
            _ => Self::Tuple(iter),
        })
    }
}

/// Implements a tuple projection for a query plan
pub enum ProjectIter<'a> {
    None(Iter<'a>),
    Some(Iter<'a>, usize),
}

impl<'a> Iterator for ProjectIter<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None(iter) => iter.find_map(|tuple| {
                if let Tuple::Row(ptr) = tuple {
                    return Some(ptr);
                }
                None
            }),
            Self::Some(iter, i) => iter.find_map(|tuple| tuple.select(*i)),
        }
    }
}

impl<'a> ProjectIter<'a> {
    pub fn build<Tx>(plan: &'a ProjectPlan, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        match plan {
            ProjectPlan::None(plan) | ProjectPlan::Name(plan, _, None) => Iter::build(plan, tx).map(Self::None),
            ProjectPlan::Name(plan, _, Some(i)) => Iter::build(plan, tx).map(|iter| Self::Some(iter, *i)),
        }
    }
}

/// A generic tuple-at-a-time iterator for a query plan
pub enum Iter<'a> {
    Row(RowRefIter<'a>),
    Join(LeftDeepJoinIter<'a>),
    Filter(Filter<'a, Iter<'a>>),
}

impl<'a> Iterator for Iter<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Row(iter) => iter.next().map(Tuple::Row),
            Self::Join(iter) => iter.next(),
            Self::Filter(iter) => iter.next(),
        }
    }
}

impl<'a> Iter<'a> {
    fn build<Tx>(plan: &'a PhysicalPlan, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        match plan {
            PhysicalPlan::TableScan(..) | PhysicalPlan::IxScan(..) => RowRefIter::build(plan, tx).map(Self::Row),
            PhysicalPlan::Filter(input, expr) => {
                // Build a filter iterator
                Iter::build(input, tx)
                    .map(Box::new)
                    .map(|input| Filter { input, expr })
                    .map(Iter::Filter)
            }
            PhysicalPlan::NLJoin(lhs, rhs) => {
                // Build a nested loop join iterator
                NLJoin::build_from(lhs, rhs, tx)
                    .map(LeftDeepJoinIter::NLJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::IxJoin(join @ IxJoin { unique: false, .. }, Semi::Lhs) => {
                // Build a left index semijoin iterator
                IxJoinLhs::build_from(join, tx)
                    .map(SemiJoin::Lhs)
                    .map(LeftDeepJoinIter::IxJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::IxJoin(join @ IxJoin { unique: false, .. }, Semi::Rhs) => {
                // Build a right index semijoin iterator
                IxJoinRhs::build_from(join, tx)
                    .map(SemiJoin::Rhs)
                    .map(LeftDeepJoinIter::IxJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::IxJoin(join @ IxJoin { unique: false, .. }, Semi::All) => {
                // Build an index join iterator
                IxJoinIter::build_from(join, tx)
                    .map(SemiJoin::All)
                    .map(LeftDeepJoinIter::IxJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::IxJoin(join @ IxJoin { unique: true, .. }, Semi::Lhs) => {
                // Build a unique left index semijoin iterator
                UniqueIxJoinLhs::build_from(join, tx)
                    .map(SemiJoin::Lhs)
                    .map(LeftDeepJoinIter::UniqueIxJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::IxJoin(join @ IxJoin { unique: true, .. }, Semi::Rhs) => {
                // Build a unique right index semijoin iterator
                UniqueIxJoinRhs::build_from(join, tx)
                    .map(SemiJoin::Rhs)
                    .map(LeftDeepJoinIter::UniqueIxJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::IxJoin(join @ IxJoin { unique: true, .. }, Semi::All) => {
                // Build a unique index join iterator
                UniqueIxJoin::build_from(join, tx)
                    .map(SemiJoin::All)
                    .map(LeftDeepJoinIter::UniqueIxJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::HashJoin(join @ HashJoin { unique: false, .. }, Semi::Lhs) => {
                // Build a left hash semijoin iterator
                HashJoinLhs::build_from(join, tx)
                    .map(SemiJoin::Lhs)
                    .map(LeftDeepJoinIter::HashJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::HashJoin(join @ HashJoin { unique: false, .. }, Semi::Rhs) => {
                // Build a right hash semijoin iterator
                HashJoinRhs::build_from(join, tx)
                    .map(SemiJoin::Rhs)
                    .map(LeftDeepJoinIter::HashJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::HashJoin(join @ HashJoin { unique: false, .. }, Semi::All) => {
                // Build a hash join iterator
                HashJoinIter::build_from(join, tx)
                    .map(SemiJoin::All)
                    .map(LeftDeepJoinIter::HashJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::HashJoin(join @ HashJoin { unique: true, .. }, Semi::Lhs) => {
                // Build a unique left hash semijoin iterator
                UniqueHashJoinLhs::build_from(join, tx)
                    .map(SemiJoin::Lhs)
                    .map(LeftDeepJoinIter::UniqueHashJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::HashJoin(join @ HashJoin { unique: true, .. }, Semi::Rhs) => {
                // Build a unique right hash semijoin iterator
                UniqueHashJoinRhs::build_from(join, tx)
                    .map(SemiJoin::Rhs)
                    .map(LeftDeepJoinIter::UniqueHashJoin)
                    .map(Iter::Join)
            }
            PhysicalPlan::HashJoin(join @ HashJoin { unique: true, .. }, Semi::All) => {
                // Build a unique hash join iterator
                UniqueHashJoin::build_from(join, tx)
                    .map(SemiJoin::All)
                    .map(LeftDeepJoinIter::UniqueHashJoin)
                    .map(Iter::Join)
            }
        }
    }
}

/// An iterator that always returns [RowRef]s
pub enum RowRefIter<'a> {
    TableScan(TableScanIter<'a>),
    IndexScan(IndexScanIter<'a>),
    DeltaScan(DeltaScanIter<'a>),
    RowFilter(Filter<'a, RowRefIter<'a>>),
}

impl<'a> Iterator for RowRefIter<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::TableScan(iter) => iter.next().map(Row::Ptr),
            Self::IndexScan(iter) => iter.next().map(Row::Ptr),
            Self::DeltaScan(iter) => iter.next().map(Row::Ref),
            Self::RowFilter(iter) => iter.next(),
        }
    }
}

impl<'a> RowRefIter<'a> {
    /// Instantiate an iterator from a [PhysicalPlan].
    /// The compiler ensures this isn't called on a join.
    fn build<Tx>(plan: &'a PhysicalPlan, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let concat = |prefix: &[(_, AlgebraicValue)], v| {
            ProductValue::from_iter(prefix.iter().map(|(_, v)| v).chain([v]).cloned())
        };
        match plan {
            PhysicalPlan::TableScan(schema, _, None) => tx.table_scan(schema.table_id).map(Self::TableScan),
            PhysicalPlan::TableScan(schema, _, Some(Delta::Inserts(..))) => {
                tx.delta_scan(schema.table_id, true).map(Self::DeltaScan)
            }
            PhysicalPlan::TableScan(schema, _, Some(Delta::Deletes(..))) => {
                tx.delta_scan(schema.table_id, false).map(Self::DeltaScan)
            }
            PhysicalPlan::IxScan(
                scan @ IxScan {
                    arg: Sarg::Eq(_, v), ..
                },
                _,
            ) if scan.prefix.is_empty() => tx
                .index_scan(scan.schema.table_id, scan.index_id, v)
                .map(Self::IndexScan),
            PhysicalPlan::IxScan(
                scan @ IxScan {
                    arg: Sarg::Eq(_, v), ..
                },
                _,
            ) => tx
                .index_scan(
                    scan.schema.table_id,
                    scan.index_id,
                    &AlgebraicValue::product(concat(&scan.prefix, v)),
                )
                .map(Self::IndexScan),
            PhysicalPlan::IxScan(
                scan @ IxScan {
                    arg: Sarg::Range(_, lower, upper),
                    ..
                },
                _,
            ) if scan.prefix.is_empty() => tx
                .index_scan(scan.schema.table_id, scan.index_id, &(lower.as_ref(), upper.as_ref()))
                .map(Self::IndexScan),
            PhysicalPlan::IxScan(
                scan @ IxScan {
                    arg: Sarg::Range(_, lower, upper),
                    ..
                },
                _,
            ) => tx
                .index_scan(
                    scan.schema.table_id,
                    scan.index_id,
                    &(
                        lower
                            .as_ref()
                            .map(|v| concat(&scan.prefix, v))
                            .map(AlgebraicValue::Product),
                        upper
                            .as_ref()
                            .map(|v| concat(&scan.prefix, v))
                            .map(AlgebraicValue::Product),
                    ),
                )
                .map(Self::IndexScan),
            PhysicalPlan::Filter(input, expr) => Self::build(input, tx)
                .map(Box::new)
                .map(|input| Filter { input, expr })
                .map(Self::RowFilter),
            _ => bail!("Plan does not return row ids"),
        }
    }
}

/// An iterator for a left deep join tree.
///
/// ```text
///     x
///    / \
///   x   c
///  / \
/// a   b
/// ```
pub enum LeftDeepJoinIter<'a> {
    /// A nested loop join
    NLJoin(NLJoin<'a>),
    /// An index join
    IxJoin(SemiJoin<IxJoinIter<'a>, IxJoinLhs<'a>, IxJoinRhs<'a>>),
    /// An index join for a unique constraint
    UniqueIxJoin(SemiJoin<UniqueIxJoin<'a>, UniqueIxJoinLhs<'a>, UniqueIxJoinRhs<'a>>),
    /// A hash join
    HashJoin(SemiJoin<HashJoinIter<'a>, HashJoinLhs<'a>, HashJoinRhs<'a>>),
    /// A hash join for a unique constraint
    UniqueHashJoin(SemiJoin<UniqueHashJoin<'a>, UniqueHashJoinLhs<'a>, UniqueHashJoinRhs<'a>>),
}

impl<'a> Iterator for LeftDeepJoinIter<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::NLJoin(iter) => iter.next().map(|(tuple, rhs)| tuple.append(rhs)),
            Self::IxJoin(iter) => iter.next(),
            Self::UniqueIxJoin(iter) => iter.next(),
            Self::HashJoin(iter) => iter.next(),
            Self::UniqueHashJoin(iter) => iter.next(),
        }
    }
}

/// A semijoin iterator.
/// Returns [RowRef]s if this is a right semijoin.
/// Returns [Tuple]s otherwise.
pub enum SemiJoin<All, Lhs, Rhs> {
    All(All),
    Lhs(Lhs),
    Rhs(Rhs),
}

impl<'a, All, Lhs, Rhs> Iterator for SemiJoin<All, Lhs, Rhs>
where
    All: Iterator<Item = (Tuple<'a>, Row<'a>)>,
    Lhs: Iterator<Item = Tuple<'a>>,
    Rhs: Iterator<Item = Row<'a>>,
{
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::All(iter) => iter.next().map(|(tuple, ptr)| tuple.append(ptr)),
            Self::Lhs(iter) => iter.next(),
            Self::Rhs(iter) => iter.next().map(Tuple::Row),
        }
    }
}

/// An index join that uses a unique constraint index
pub struct UniqueIxJoin<'a> {
    /// The lhs of the join
    lhs: Box<Iter<'a>>,
    /// The rhs index
    rhs_index: &'a BTreeIndex,
    /// A handle to the datastore
    rhs_table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> UniqueIxJoin<'a> {
    fn build_from<Tx>(join: &'a IxJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_table = tx.table_or_err(join.rhs.table_id)?;
        let rhs_index = rhs_table
            .get_index_by_id(join.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{}` does not exist", join.rhs_index))?;
        Ok(Self {
            lhs: Box::new(lhs),
            rhs_index,
            rhs_table,
            blob_store: tx.blob_store(),
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for UniqueIxJoin<'a> {
    type Item = (Tuple<'a>, Row<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs.find_map(|tuple| {
            self.rhs_index
                .seek(&tuple.project(self.lhs_field))
                .next()
                .and_then(|ptr| self.rhs_table.get_row_ref(self.blob_store, ptr))
                .map(Row::Ptr)
                .map(|ptr| (tuple, ptr))
        })
    }
}

/// A left semijoin that uses a unique constraint index
pub struct UniqueIxJoinLhs<'a> {
    /// The lhs of the join
    lhs: Box<Iter<'a>>,
    /// The rhs index
    rhs: &'a BTreeIndex,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> UniqueIxJoinLhs<'a> {
    fn build_from<Tx>(join: &'a IxJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_table = tx.table_or_err(join.rhs.table_id)?;
        let rhs_index = rhs_table
            .get_index_by_id(join.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{}` does not exist", join.rhs_index))?;
        Ok(Self {
            lhs: Box::new(lhs),
            rhs: rhs_index,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for UniqueIxJoinLhs<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs.find(|t| self.rhs.contains_any(&t.project(self.lhs_field)))
    }
}

/// A right semijoin that uses a unique constraint index
pub struct UniqueIxJoinRhs<'a> {
    /// The lhs of the join
    lhs: Box<Iter<'a>>,
    /// The rhs index
    rhs_index: &'a BTreeIndex,
    /// A handle to the datastore
    rhs_table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> UniqueIxJoinRhs<'a> {
    fn build_from<Tx>(join: &'a IxJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_table = tx.table_or_err(join.rhs.table_id)?;
        let rhs_index = rhs_table
            .get_index_by_id(join.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{}` does not exist", join.rhs_index))?;
        Ok(Self {
            lhs: Box::new(lhs),
            rhs_index,
            rhs_table,
            blob_store: tx.blob_store(),
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for UniqueIxJoinRhs<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs.find_map(|tuple| {
            self.rhs_index
                .seek(&tuple.project(self.lhs_field))
                .next()
                .and_then(|ptr| self.rhs_table.get_row_ref(self.blob_store, ptr))
                .map(Row::Ptr)
        })
    }
}

/// An index join that does not use a unique constraint index
pub struct IxJoinIter<'a> {
    /// The lhs of the join
    lhs: Box<Iter<'a>>,
    /// The current lhs tuple
    lhs_tuple: Option<Tuple<'a>>,
    /// The rhs index
    rhs_index: &'a BTreeIndex,
    /// The current rhs index cursor
    rhs_index_cursor: Option<BTreeIndexRangeIter<'a>>,
    /// A handle to the datastore
    rhs_table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> IxJoinIter<'a> {
    fn build_from<Tx>(join: &'a IxJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_table = tx.table_or_err(join.rhs.table_id)?;
        let rhs_index = rhs_table
            .get_index_by_id(join.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{}` does not exist", join.rhs_index))?;
        Ok(Self {
            lhs: Box::new(lhs),
            lhs_tuple: None,
            rhs_index,
            rhs_index_cursor: None,
            rhs_table,
            blob_store: tx.blob_store(),
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for IxJoinIter<'a> {
    type Item = (Tuple<'a>, Row<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs_tuple
            .as_ref()
            .and_then(|tuple| {
                self.rhs_index_cursor.as_mut().and_then(|cursor| {
                    cursor.next().and_then(|ptr| {
                        self.rhs_table
                            .get_row_ref(self.blob_store, ptr)
                            .map(Row::Ptr)
                            .map(|ptr| (tuple.clone(), ptr))
                    })
                })
            })
            .or_else(|| {
                self.lhs.find_map(|tuple| {
                    let mut cursor = self.rhs_index.seek(&tuple.project(self.lhs_field));
                    cursor.next().and_then(|ptr| {
                        self.rhs_table
                            .get_row_ref(self.blob_store, ptr)
                            .map(Row::Ptr)
                            .map(|ptr| {
                                self.lhs_tuple = Some(tuple.clone());
                                self.rhs_index_cursor = Some(cursor);
                                (tuple, ptr)
                            })
                    })
                })
            })
    }
}

/// A left semijoin that does not use a unique constraint index
pub struct IxJoinLhs<'a> {
    /// The lhs of the join
    lhs: Box<Iter<'a>>,
    /// The rhs index
    rhs_index: &'a BTreeIndex,
    /// The current lhs tuple
    lhs_tuple: Option<Tuple<'a>>,
    /// The matching rhs row count
    rhs_count: usize,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> IxJoinLhs<'a> {
    fn build_from<Tx>(join: &'a IxJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_table = tx.table_or_err(join.rhs.table_id)?;
        let rhs_index = rhs_table
            .get_index_by_id(join.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{}` does not exist", join.rhs_index))?;
        Ok(Self {
            lhs: Box::new(lhs),
            lhs_tuple: None,
            rhs_count: 0,
            rhs_index,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for IxJoinLhs<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rhs_count {
            0 => self
                .lhs
                .find_map(|tuple| self.rhs_index.count(&tuple.project(self.lhs_field)).map(|n| (tuple, n)))
                .map(|(tuple, n)| {
                    self.rhs_count = n - 1;
                    self.lhs_tuple = Some(tuple.clone());
                    tuple
                }),
            _ => {
                self.rhs_count -= 1;
                self.lhs_tuple.clone()
            }
        }
    }
}

/// A right semijoin that does not use a unique constraint index
pub struct IxJoinRhs<'a> {
    /// The lhs of the join
    lhs: Box<Iter<'a>>,
    /// The rhs index
    rhs_index: &'a BTreeIndex,
    /// The current rhs index cursor
    rhs_index_cursor: Option<BTreeIndexRangeIter<'a>>,
    /// A handle to the datastore
    rhs_table: &'a Table,
    /// A handle to the blobstore
    blob_store: &'a dyn BlobStore,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> IxJoinRhs<'a> {
    fn build_from<Tx>(join: &'a IxJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_table = tx.table_or_err(join.rhs.table_id)?;
        let rhs_index = rhs_table
            .get_index_by_id(join.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{}` does not exist", join.rhs_index))?;
        Ok(Self {
            lhs: Box::new(lhs),
            rhs_index,
            rhs_index_cursor: None,
            rhs_table,
            blob_store: tx.blob_store(),
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for IxJoinRhs<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.rhs_index_cursor
            .as_mut()
            .and_then(|cursor| {
                cursor
                    .next()
                    .and_then(|ptr| self.rhs_table.get_row_ref(self.blob_store, ptr))
                    .map(Row::Ptr)
            })
            .or_else(|| {
                self.lhs.find_map(|tuple| {
                    let mut cursor = self.rhs_index.seek(&tuple.project(self.lhs_field));
                    cursor.next().and_then(|ptr| {
                        self.rhs_table
                            .get_row_ref(self.blob_store, ptr)
                            .map(Row::Ptr)
                            .inspect(|_| self.rhs_index_cursor = Some(cursor))
                    })
                })
            })
    }
}

/// A hash join that on each probe,
/// returns at most one row from the hash table.
pub struct UniqueHashJoin<'a> {
    /// The lhs relation
    lhs: Box<Iter<'a>>,
    /// The rhs hash table
    rhs: HashMap<AlgebraicValue, Row<'a>>,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> UniqueHashJoin<'a> {
    /// Builds a hash table over the rhs
    fn build_from<Tx>(join: &'a HashJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs = RowRefIter::build(&join.rhs, tx)?;
        let rhs = rhs.map(|ptr| (ptr.project(&join.rhs_field), ptr)).collect();
        Ok(Self {
            lhs: Box::new(lhs),
            rhs,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for UniqueHashJoin<'a> {
    type Item = (Tuple<'a>, Row<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs.find_map(|tuple| {
            self.rhs
                .get(&tuple.project(self.lhs_field))
                .cloned()
                .map(|ptr| (tuple, ptr))
        })
    }
}

/// A left hash semijoin that on each probe,
/// returns at most one row from the hash table.
pub struct UniqueHashJoinLhs<'a> {
    /// The lhs relation
    lhs: Box<Iter<'a>>,
    /// The rhs hash table
    rhs: HashSet<AlgebraicValue>,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> UniqueHashJoinLhs<'a> {
    /// Builds a hash set over the rhs
    fn build_from<Tx>(join: &'a HashJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs = RowRefIter::build(&join.rhs, tx)?;
        let rhs = rhs.map(|ptr| ptr.project(&join.rhs_field)).collect();
        Ok(Self {
            lhs: Box::new(lhs),
            rhs,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for UniqueHashJoinLhs<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs.find(|t| self.rhs.contains(&t.project(self.lhs_field)))
    }
}

/// A right hash join that on each probe,
/// returns at most one row from the hash table.
pub struct UniqueHashJoinRhs<'a> {
    /// The lhs relation
    lhs: Box<Iter<'a>>,
    /// The rhs hash table
    rhs: HashMap<AlgebraicValue, Row<'a>>,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> UniqueHashJoinRhs<'a> {
    /// Builds a hash table over the rhs
    fn build_from<Tx>(join: &'a HashJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs = RowRefIter::build(&join.rhs, tx)?;
        let rhs = rhs.map(|ptr| (ptr.project(&join.rhs_field), ptr)).collect();
        Ok(Self {
            lhs: Box::new(lhs),
            rhs,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for UniqueHashJoinRhs<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs.find_map(|t| self.rhs.get(&t.project(self.lhs_field)).cloned())
    }
}

/// A hash join that on each probe,
/// may return many rows from the hash table.
pub struct HashJoinIter<'a> {
    /// The lhs relation
    lhs: Box<Iter<'a>>,
    /// The rhs hash table
    rhs: HashMap<AlgebraicValue, Vec<Row<'a>>>,
    /// The current lhs tuple
    lhs_tuple: Option<Tuple<'a>>,
    /// The current rhs row pointer
    rhs_ptr: usize,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> HashJoinIter<'a> {
    /// Builds a hash table over the rhs
    fn build_from<Tx>(join: &'a HashJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_iter = RowRefIter::build(&join.rhs, tx)?;
        let mut rhs = HashMap::new();
        for ptr in rhs_iter {
            let val = ptr.project(&join.rhs_field);
            match rhs.get_mut(&val) {
                None => {
                    rhs.insert(val, vec![ptr]);
                }
                Some(ptrs) => {
                    ptrs.push(ptr);
                }
            }
        }
        Ok(Self {
            lhs: Box::new(lhs),
            rhs,
            lhs_tuple: None,
            rhs_ptr: 0,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for HashJoinIter<'a> {
    type Item = (Tuple<'a>, Row<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs_tuple
            .as_ref()
            .and_then(|tuple| {
                self.rhs.get(&tuple.project(self.lhs_field)).and_then(|ptrs| {
                    let i = self.rhs_ptr;
                    self.rhs_ptr += 1;
                    ptrs.get(i).map(|ptr| (tuple.clone(), ptr.clone()))
                })
            })
            .or_else(|| {
                self.lhs.find_map(|tuple| {
                    self.rhs.get(&tuple.project(self.lhs_field)).and_then(|ptrs| {
                        self.rhs_ptr = 1;
                        self.lhs_tuple = Some(tuple.clone());
                        ptrs.first().map(|ptr| (tuple, ptr.clone()))
                    })
                })
            })
    }
}

/// A left hash semijoin that on each probe,
/// may return many rows from the hash table.
pub struct HashJoinLhs<'a> {
    /// The lhs relation
    lhs: Box<Iter<'a>>,
    /// The rhs hash table
    rhs: HashMap<AlgebraicValue, usize>,
    /// The current lhs tuple
    lhs_tuple: Option<Tuple<'a>>,
    /// The matching number of rhs rows
    rhs_count: usize,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> HashJoinLhs<'a> {
    /// Instantiates the iterator by building a hash table over the rhs
    fn build_from<Tx>(join: &'a HashJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_iter = RowRefIter::build(&join.rhs, tx)?;
        let mut rhs = HashMap::new();
        for ptr in rhs_iter {
            rhs.entry(ptr.project(&join.rhs_field))
                .and_modify(|n| *n += 1)
                .or_insert_with(|| 1);
        }
        Ok(Self {
            lhs: Box::new(lhs),
            rhs,
            lhs_tuple: None,
            rhs_count: 0,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for HashJoinLhs<'a> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rhs_count {
            0 => self.lhs.find_map(|tuple| {
                self.rhs.get(&tuple.project(self.lhs_field)).map(|n| {
                    self.rhs_count = *n - 1;
                    self.lhs_tuple = Some(tuple.clone());
                    tuple
                })
            }),
            _ => {
                self.rhs_count -= 1;
                self.lhs_tuple.clone()
            }
        }
    }
}

/// A right hash semijoin that on each probe,
/// may return many rows from the hash table.
pub struct HashJoinRhs<'a> {
    /// The lhs relation
    lhs: Box<Iter<'a>>,
    /// The rhs hash table
    rhs: HashMap<AlgebraicValue, Vec<Row<'a>>>,
    /// The current lhs tuple
    lhs_value: Option<AlgebraicValue>,
    /// The current rhs row pointer
    rhs_ptr: usize,
    /// The lhs probe field
    lhs_field: &'a TupleField,
}

impl<'a> HashJoinRhs<'a> {
    /// Builds a hash table over the rhs
    fn build_from<Tx>(join: &'a HashJoin, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(&join.lhs, tx)?;
        let rhs_iter = RowRefIter::build(&join.rhs, tx)?;
        let mut rhs = HashMap::new();
        for ptr in rhs_iter {
            let val = ptr.project(&join.rhs_field);
            match rhs.get_mut(&val) {
                None => {
                    rhs.insert(val, vec![ptr]);
                }
                Some(ptrs) => {
                    ptrs.push(ptr);
                }
            }
        }
        Ok(Self {
            lhs: Box::new(lhs),
            rhs,
            lhs_value: None,
            rhs_ptr: 0,
            lhs_field: &join.lhs_field,
        })
    }
}

impl<'a> Iterator for HashJoinRhs<'a> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lhs_value
            .as_ref()
            .and_then(|value| {
                self.rhs.get(value).and_then(|ptrs| {
                    let i = self.rhs_ptr;
                    self.rhs_ptr += 1;
                    ptrs.get(i).cloned()
                })
            })
            .or_else(|| {
                self.lhs.find_map(|tuple| {
                    let value = tuple.project(self.lhs_field);
                    self.rhs.get(&value).and_then(|ptrs| {
                        self.rhs_ptr = 1;
                        self.lhs_value = Some(value.clone());
                        ptrs.first().cloned()
                    })
                })
            })
    }
}

/// A nested loop join iterator
pub struct NLJoin<'a> {
    /// The lhs input
    lhs: Box<Iter<'a>>,
    /// The materialized rhs
    rhs: Vec<Row<'a>>,
    /// The current lhs tuple
    lhs_tuple: Option<Tuple<'a>>,
    /// The current rhs row pointer
    rhs_ptr: usize,
}

impl<'a> NLJoin<'a> {
    /// Instantiates the iterator by materializing the rhs
    fn build_from<Tx>(lhs: &'a PhysicalPlan, rhs: &'a PhysicalPlan, tx: &'a Tx) -> Result<Self>
    where
        Tx: Datastore + DeltaStore,
    {
        let lhs = Iter::build(lhs, tx)?;
        let rhs = RowRefIter::build(rhs, tx)?;
        Ok(Self {
            lhs: Box::new(lhs),
            rhs: rhs.collect(),
            lhs_tuple: None,
            rhs_ptr: 0,
        })
    }
}

impl<'a> Iterator for NLJoin<'a> {
    type Item = (Tuple<'a>, Row<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.rhs.get(self.rhs_ptr) {
            Some(v) => {
                self.rhs_ptr += 1;
                self.lhs_tuple.as_ref().map(|u| (u.clone(), v.clone()))
            }
            None => {
                self.rhs_ptr = 1;
                self.lhs_tuple = self.lhs.next();
                self.lhs_tuple
                    .as_ref()
                    .zip(self.rhs.first())
                    .map(|(u, v)| (u.clone(), v.clone()))
            }
        }
    }
}

/// A tuple-at-a-time filter iterator
pub struct Filter<'a, I> {
    input: Box<I>,
    expr: &'a PhysicalExpr,
}

impl<'a> Iterator for Filter<'a, RowRefIter<'a>> {
    type Item = Row<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.find(|ptr| self.expr.eval_bool(ptr))
    }
}

impl<'a> Iterator for Filter<'a, Iter<'a>> {
    type Item = Tuple<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.find(|tuple| self.expr.eval_bool(tuple))
    }
}
