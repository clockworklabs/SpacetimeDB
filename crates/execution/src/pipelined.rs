use std::{
    collections::{HashMap, HashSet},
    ops::Bound,
};

use anyhow::{anyhow, Result};
use itertools::Either;
use spacetimedb_expr::expr::AggType;
use spacetimedb_lib::{metrics::ExecutionMetrics, query::Delta, sats::size_of::SizeOf, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::plan::{
    HashJoin, IxJoin, IxScan, PhysicalExpr, PhysicalPlan, ProjectField, ProjectListPlan, ProjectPlan, Sarg, Semi,
    TableScan, TupleField,
};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::product;

use crate::{Datastore, DeltaStore, Row, Tuple};

/// An executor for explicit column projections.
/// Note, this plan can only be constructed from the http api,
/// which is not considered performance critical.
/// Hence this operator is not particularly optimized.
pub enum ProjectListExecutor {
    Name(Vec<PipelinedProject>),
    List(Vec<PipelinedExecutor>, Vec<TupleField>),
    Limit(Box<ProjectListExecutor>, u64),
    Agg(Vec<PipelinedExecutor>, AggType),
}

impl From<ProjectListPlan> for ProjectListExecutor {
    fn from(plan: ProjectListPlan) -> Self {
        match plan {
            ProjectListPlan::Name(plan) => Self::Name(plan.into_iter().map(PipelinedProject::from).collect()),
            ProjectListPlan::List(plan, fields) => {
                Self::List(plan.into_iter().map(PipelinedExecutor::from).collect(), fields)
            }
            ProjectListPlan::Limit(plan, n) => Self::Limit(Box::new((*plan).into()), n),
            ProjectListPlan::Agg(plan, AggType::Count) => {
                Self::Agg(plan.into_iter().map(PipelinedExecutor::from).collect(), AggType::Count)
            }
        }
    }
}

impl ProjectListExecutor {
    pub fn execute<Tx: Datastore + DeltaStore>(
        &self,
        tx: &Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(ProductValue) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut bytes_scanned = 0;
        match self {
            Self::Name(plans) => {
                for plan in plans {
                    plan.execute(tx, metrics, &mut |row| {
                        n += 1;
                        let row = row.to_product_value();
                        bytes_scanned += row.size_of();
                        f(row)
                    })?;
                }
            }
            Self::List(plans, fields) => {
                for plan in plans {
                    plan.execute(tx, metrics, &mut |t| {
                        n += 1;
                        let row = ProductValue::from_iter(fields.iter().map(|field| t.project(field)));
                        bytes_scanned += row.size_of();
                        f(row)
                    })?;
                }
            }
            Self::Limit(plan, limit) => {
                plan.execute(tx, metrics, &mut |row| {
                    n += 1;
                    if n <= *limit as usize {
                        f(row)?;
                    }
                    Ok(())
                })?;
            }
            Self::Agg(plans, AggType::Count) => {
                for plan in plans {
                    match plan {
                        // TODO: This is a hack that needs to be removed.
                        // We check if this is a COUNT on a physical table,
                        // and if so, we retrieve the count from table metadata.
                        // It's a valid optimization but one that should be done by the optimizer.
                        // There should be no optimizations performed during execution.
                        PipelinedExecutor::TableScan(table_scan) => {
                            n += tx.table_or_err(table_scan.table)?.num_rows() as usize;
                        }
                        _ => {
                            plan.execute(tx, metrics, &mut |_| {
                                n += 1;
                                Ok(())
                            })?;
                        }
                    }
                }
                f(product![n as u64])?;
            }
        }
        metrics.rows_scanned += n;
        metrics.bytes_scanned += bytes_scanned;
        Ok(())
    }
}

/// Implements a projection on top of a pipelined executor
#[derive(Debug)]
pub enum PipelinedProject {
    None(PipelinedExecutor),
    Some(PipelinedExecutor, usize),
}

impl From<ProjectPlan> for PipelinedProject {
    fn from(plan: ProjectPlan) -> Self {
        match plan {
            ProjectPlan::None(plan) => Self::None(plan.into()),
            ProjectPlan::Name(plan, _, None) => Self::None(plan.into()),
            ProjectPlan::Name(plan, _, Some(i)) => Self::Some(plan.into(), i),
        }
    }
}

impl PipelinedProject {
    /// Walks and visits each executor in the tree
    pub fn visit(&self, f: &mut impl FnMut(&PipelinedExecutor)) {
        match self {
            Self::Some(plan, _) | Self::None(plan) => {
                plan.visit(f);
            }
        }
    }

    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        match self {
            Self::None(plan) | Self::Some(plan, _) => plan.is_empty(tx),
        }
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Row<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        match self {
            Self::None(plan) => {
                // No explicit projection.
                // This means the input does not return tuples.
                // It returns either row ids or product values.
                plan.execute(tx, metrics, &mut |t| {
                    n += 1;
                    if let Tuple::Row(row) = t {
                        f(row)?;
                    }
                    Ok(())
                })?;
            }
            Self::Some(plan, i) => {
                // The contrary is true for explicit projections.
                // They return a tuple of row ids or product values.
                plan.execute(tx, metrics, &mut |t| {
                    n += 1;
                    if let Some(row) = t.select(*i) {
                        f(row)?;
                    }
                    Ok(())
                })?;
            }
        }
        metrics.rows_scanned += n;
        Ok(())
    }
}

/// Executes a query plan in a streaming fashion.
/// Avoids materializing intermediate results when possible.
/// Note that unlike a tuple at a time iterator,
/// the caller has no way to interrupt its forward progress.
#[derive(Debug)]
pub enum PipelinedExecutor {
    TableScan(PipelinedScan),
    IxScan(PipelinedIxScan),
    IxJoin(PipelinedIxJoin),
    IxDeltaScan(PipelinedIxDeltaScan),
    IxDeltaJoin(PipelinedIxDeltaJoin),
    HashJoin(BlockingHashJoin),
    NLJoin(BlockingNLJoin),
    Filter(PipelinedFilter),
    Limit(PipelinedLimit),
}

impl From<PhysicalPlan> for PipelinedExecutor {
    fn from(plan: PhysicalPlan) -> Self {
        match plan {
            PhysicalPlan::TableScan(TableScan { schema, limit, delta }, _) => Self::TableScan(PipelinedScan {
                table: schema.table_id,
                limit,
                delta,
            }),
            PhysicalPlan::IxScan(scan @ IxScan { delta: None, .. }, _) => Self::IxScan(scan.into()),
            PhysicalPlan::IxScan(scan, _) => Self::IxDeltaScan(scan.into()),
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_index,
                    rhs_field,
                    unique,
                    lhs_field,
                    rhs_delta: None,
                    ..
                },
                semijoin,
            ) => Self::IxJoin(PipelinedIxJoin {
                lhs: Box::new(Self::from(*lhs)),
                rhs_table: rhs.table_id,
                rhs_index,
                rhs_field,
                lhs_field,
                unique,
                semijoin,
            }),
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_index,
                    rhs_field,
                    unique,
                    lhs_field,
                    rhs_delta: Some(rhs_delta),
                    ..
                },
                semijoin,
            ) => Self::IxDeltaJoin(PipelinedIxDeltaJoin {
                lhs: Box::new(Self::from(*lhs)),
                rhs_table: rhs.table_id,
                rhs_index,
                rhs_field,
                rhs_delta,
                lhs_field,
                unique,
                semijoin,
            }),
            PhysicalPlan::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field,
                    rhs_field,
                    unique,
                },
                semijoin,
            ) => Self::HashJoin(BlockingHashJoin {
                lhs: Box::new(PipelinedExecutor::from(*lhs)),
                rhs: Box::new(PipelinedExecutor::from(*rhs)),
                lhs_field,
                rhs_field,
                unique,
                semijoin,
            }),
            PhysicalPlan::NLJoin(lhs, rhs) => Self::NLJoin(BlockingNLJoin {
                lhs: Box::new(PipelinedExecutor::from(*lhs)),
                rhs: Box::new(PipelinedExecutor::from(*rhs)),
            }),
            PhysicalPlan::Filter(input, expr) => Self::Filter(PipelinedFilter {
                input: Box::new(PipelinedExecutor::from(*input)),
                expr,
            }),
        }
    }
}

impl PipelinedExecutor {
    /// Walks and visits each executor in the tree
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::IxJoin(PipelinedIxJoin { lhs: input, .. })
            | Self::IxDeltaJoin(PipelinedIxDeltaJoin { lhs: input, .. })
            | Self::Filter(PipelinedFilter { input, .. })
            | Self::Limit(PipelinedLimit { input, .. }) => {
                input.visit(f);
            }
            Self::NLJoin(BlockingNLJoin { lhs, rhs }) | Self::HashJoin(BlockingHashJoin { lhs, rhs, .. }) => {
                lhs.visit(f);
                rhs.visit(f);
            }
            Self::TableScan(..) | Self::IxScan(..) | Self::IxDeltaScan(..) => {}
        }
    }

    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        match self {
            Self::TableScan(scan) => scan.is_empty(tx),
            Self::IxScan(scan) => scan.is_empty(tx),
            Self::IxDeltaScan(scan) => scan.is_empty(tx),
            Self::IxJoin(join) => join.is_empty(tx),
            Self::IxDeltaJoin(join) => join.is_empty(tx),
            Self::HashJoin(join) => join.is_empty(tx),
            Self::NLJoin(join) => join.is_empty(tx),
            Self::Filter(filter) => filter.is_empty(tx),
            Self::Limit(limit) => limit.is_empty(tx),
        }
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::TableScan(scan) => scan.execute(tx, metrics, f),
            Self::IxScan(scan) => scan.execute(tx, metrics, f),
            Self::IxDeltaScan(scan) => scan.execute(tx, metrics, f),
            Self::IxJoin(join) => join.execute(tx, metrics, f),
            Self::IxDeltaJoin(join) => join.execute(tx, metrics, f),
            Self::HashJoin(join) => join.execute(tx, metrics, f),
            Self::NLJoin(join) => join.execute(tx, metrics, f),
            Self::Filter(filter) => filter.execute(tx, metrics, f),
            Self::Limit(limit) => limit.execute(tx, metrics, f),
        }
    }
}

/// A pipelined executor for scanning both physical and delta tables
#[derive(Debug)]
pub struct PipelinedScan {
    pub table: TableId,
    pub limit: Option<u64>,
    pub delta: Option<Delta>,
}

impl PipelinedScan {
    /// Is this an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        match self.delta {
            Some(Delta::Inserts) => !tx.has_inserts(self.table),
            Some(Delta::Deletes) => !tx.has_deletes(self.table),
            None => false,
        }
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        // A physical table scan
        let table_scan = || tx.table_scan(self.table);
        // A physical table scan with optional row limit
        let table_limit_scan = |limit| match limit {
            None => table_scan().map(Either::Left),
            Some(n) => table_scan().map(|iter| iter.take(n)).map(Either::Right),
        };
        // A delta table scan
        let delta_scan = |inserts| tx.delta_scan(self.table, inserts);
        // A delta table scan with optional row limit
        let delta_limit_scan = |limit, inserts| match limit {
            None => Either::Left(delta_scan(inserts)),
            Some(n) => Either::Right(delta_scan(inserts).take(n)),
        };
        let mut n = 0;
        let mut f = |t| {
            n += 1;
            f(t)
        };
        match self.delta {
            None => {
                for tuple in table_limit_scan(self.limit.map(|n| n as usize))?
                    .map(Row::Ptr)
                    .map(Tuple::Row)
                {
                    f(tuple)?;
                }
            }
            Some(Delta::Inserts) => {
                for tuple in delta_limit_scan(self.limit.map(|n| n as usize), true)
                    .map(Row::Ref)
                    .map(Tuple::Row)
                {
                    f(tuple)?;
                }
            }
            Some(Delta::Deletes) => {
                for tuple in delta_limit_scan(self.limit.map(|n| n as usize), false)
                    .map(Row::Ref)
                    .map(Tuple::Row)
                {
                    f(tuple)?;
                }
            }
        }
        metrics.rows_scanned += n;
        Ok(())
    }
}

/// An index scan executor for a delta table.
///
/// TODO: There is much overlap between this executor and [PipelinedIxScan].
/// But merging them requires merging the [Datastore] and [DeltaStore] traits,
/// since the index scan interface is right now split between both.
#[derive(Debug)]
pub struct PipelinedIxDeltaScan {
    /// The table id
    pub table_id: TableId,
    /// The index id
    pub index_id: IndexId,
    /// An equality prefix for multi-column scans
    pub prefix: Vec<AlgebraicValue>,
    /// The lower index bound
    pub lower: Bound<AlgebraicValue>,
    /// The upper index bound
    pub upper: Bound<AlgebraicValue>,
    /// Inserts or deletes?
    pub delta: Delta,
}

impl From<IxScan> for PipelinedIxDeltaScan {
    fn from(scan: IxScan) -> Self {
        match scan {
            IxScan {
                schema,
                index_id,
                prefix,
                arg: Sarg::Eq(_, v),
                delta: Some(Delta::Inserts),
                ..
            } => Self {
                table_id: schema.table_id,
                index_id,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower: Bound::Included(v.clone()),
                upper: Bound::Included(v),
                delta: Delta::Inserts,
            },
            IxScan {
                schema,
                index_id,
                prefix,
                arg: Sarg::Eq(_, v),
                ..
            } => Self {
                table_id: schema.table_id,
                index_id,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower: Bound::Included(v.clone()),
                upper: Bound::Included(v),
                delta: Delta::Deletes,
            },
            IxScan {
                schema,
                index_id,
                prefix,
                arg: Sarg::Range(_, lower, upper),
                delta: Some(Delta::Inserts),
                ..
            } => Self {
                table_id: schema.table_id,
                index_id,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower,
                upper,
                delta: Delta::Inserts,
            },
            IxScan {
                schema,
                index_id,
                prefix,
                arg: Sarg::Range(_, lower, upper),
                ..
            } => Self {
                table_id: schema.table_id,
                index_id,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower,
                upper,
                delta: Delta::Deletes,
            },
        }
    }
}

impl PipelinedIxDeltaScan {
    /// Is the delta table empty?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        match self.delta {
            Delta::Inserts => !tx.has_inserts(self.table_id),
            Delta::Deletes => !tx.has_deletes(self.table_id),
        }
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut f = |t| {
            n += 1;
            f(t)
        };
        match self.prefix.as_slice() {
            [] => {
                for ptr in tx
                    .index_scan_range_for_delta(
                        self.table_id,
                        self.index_id,
                        self.delta,
                        (self.lower.as_ref(), self.upper.as_ref()),
                    )
                    .map(Tuple::Row)
                {
                    f(ptr)?;
                }
            }
            prefix => {
                for ptr in tx
                    .index_scan_range_for_delta(
                        self.table_id,
                        self.index_id,
                        self.delta,
                        (
                            self.lower
                                .as_ref()
                                .map(std::iter::once)
                                .map(|iter| prefix.iter().chain(iter))
                                .map(|iter| iter.cloned())
                                .map(ProductValue::from_iter)
                                .map(AlgebraicValue::Product),
                            self.upper
                                .as_ref()
                                .map(std::iter::once)
                                .map(|iter| prefix.iter().chain(iter))
                                .map(|iter| iter.cloned())
                                .map(ProductValue::from_iter)
                                .map(AlgebraicValue::Product),
                        ),
                    )
                    .map(Tuple::Row)
                {
                    f(ptr)?;
                }
            }
        }
        metrics.index_seeks += 1;
        metrics.rows_scanned += n;
        Ok(())
    }
}

/// A pipelined executor for scanning an index
#[derive(Debug)]
pub struct PipelinedIxScan {
    /// The table id
    pub table_id: TableId,
    /// The index id
    pub index_id: IndexId,
    pub limit: Option<u64>,
    /// An equality prefix for multi-column scans
    pub prefix: Vec<AlgebraicValue>,
    /// The lower index bound
    pub lower: Bound<AlgebraicValue>,
    /// The upper index bound
    pub upper: Bound<AlgebraicValue>,
}

impl From<IxScan> for PipelinedIxScan {
    fn from(scan: IxScan) -> Self {
        match scan {
            IxScan {
                schema,
                limit,
                delta: _,
                index_id,
                prefix,
                arg: Sarg::Eq(_, v),
            } => Self {
                table_id: schema.table_id,
                index_id,
                limit,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower: Bound::Included(v.clone()),
                upper: Bound::Included(v),
            },
            IxScan {
                schema,
                limit,
                delta: _,
                index_id,
                prefix,
                arg: Sarg::Range(_, lower, upper),
            } => Self {
                table_id: schema.table_id,
                index_id,
                limit,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower,
                upper,
            },
        }
    }
}

impl PipelinedIxScan {
    /// We don't know statically if an index scan will return rows
    pub fn is_empty(&self, _: &impl DeltaStore) -> bool {
        false
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        // A single column index scan
        let single_col_scan = || {
            tx.index_scan_range(
                self.table_id,
                self.index_id,
                &(self.lower.as_ref(), self.upper.as_ref()),
            )
        };
        // A single column index scan with optional row limit
        let single_col_limit_scan = |limit| match limit {
            None => single_col_scan().map(Either::Left),
            Some(n) => single_col_scan().map(|iter| iter.take(n)).map(Either::Right),
        };
        // A multi-column index scan
        let multi_col_scan = |prefix: &[AlgebraicValue]| {
            tx.index_scan_range(
                self.table_id,
                self.index_id,
                &(
                    self.lower
                        .as_ref()
                        .map(std::iter::once)
                        .map(|iter| prefix.iter().chain(iter))
                        .map(|iter| iter.cloned())
                        .map(ProductValue::from_iter)
                        .map(AlgebraicValue::Product),
                    self.upper
                        .as_ref()
                        .map(std::iter::once)
                        .map(|iter| prefix.iter().chain(iter))
                        .map(|iter| iter.cloned())
                        .map(ProductValue::from_iter)
                        .map(AlgebraicValue::Product),
                ),
            )
        };
        // A multi-column index scan with optional row limit
        let multi_col_limit_scan = |prefix, limit| match limit {
            None => multi_col_scan(prefix).map(Either::Left),
            Some(n) => multi_col_scan(prefix).map(|iter| iter.take(n)).map(Either::Right),
        };
        let mut n = 0;
        let mut f = |t| {
            n += 1;
            f(t)
        };
        match self.prefix.as_slice() {
            [] => {
                for ptr in single_col_limit_scan(self.limit.map(|n| n as usize))?
                    .map(Row::Ptr)
                    .map(Tuple::Row)
                {
                    f(ptr)?;
                }
            }
            prefix => {
                for ptr in multi_col_limit_scan(prefix, self.limit.map(|n| n as usize))?
                    .map(Row::Ptr)
                    .map(Tuple::Row)
                {
                    f(ptr)?;
                }
            }
        }
        metrics.index_seeks += 1;
        metrics.rows_scanned += n;
        Ok(())
    }
}

/// A pipelined index join executor
#[derive(Debug)]
pub struct PipelinedIxJoin {
    /// The executor for the lhs of the join
    pub lhs: Box<PipelinedExecutor>,
    /// The rhs table
    pub rhs_table: TableId,
    /// The rhs index
    pub rhs_index: IndexId,
    /// The rhs join field
    pub rhs_field: ColId,
    /// The lhs join field
    pub lhs_field: TupleField,
    /// Is the index unique?
    pub unique: bool,
    /// Is this a semijoin?
    pub semijoin: Semi,
}

impl PipelinedIxJoin {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.lhs.is_empty(tx)
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let blob_store = tx.blob_store();
        let rhs_table = tx.table_or_err(self.rhs_table)?;
        let rhs_index = rhs_table
            .get_index_by_id(self.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{0}` does not exist", self.rhs_index))?;

        let mut n = 0;
        let mut index_seeks = 0;
        let mut bytes_scanned = 0;

        match self {
            Self {
                lhs,
                lhs_field,
                unique: true,
                semijoin: Semi::Lhs,
                ..
            } => {
                // Should we evaluate the lhs tuple?
                // Probe the index to see if there is a matching row.
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if rhs_index.contains_any(&project(&u, lhs_field, &mut bytes_scanned)) {
                        f(u)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: true,
                semijoin: Semi::Rhs,
                ..
            } => {
                // Probe the index and evaluate the matching rhs row
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(v) = rhs_index
                        .seek_point(&project(&u, lhs_field, &mut bytes_scanned))
                        .next()
                        .and_then(|ptr| rhs_table.get_row_ref(blob_store, ptr))
                        .map(Row::Ptr)
                        .map(Tuple::Row)
                    {
                        f(v)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: true,
                semijoin: Semi::All,
                ..
            } => {
                // Probe the index and evaluate the matching rhs row
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(v) = rhs_index
                        .seek_point(&project(&u, lhs_field, &mut bytes_scanned))
                        .next()
                        .and_then(|ptr| rhs_table.get_row_ref(blob_store, ptr))
                        .map(Row::Ptr)
                        .map(Tuple::Row)
                    {
                        f(u.join(v))?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: false,
                semijoin: Semi::Lhs,
                ..
            } => {
                // How many times should we evaluate the lhs tuple?
                // Probe the index for the number of matching rows.
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(n) = rhs_index.count(&project(&u, lhs_field, &mut bytes_scanned)) {
                        for _ in 0..n {
                            f(u.clone())?;
                        }
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: false,
                semijoin: Semi::Rhs,
                ..
            } => {
                // Probe the index and evaluate the matching rhs rows
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    for v in rhs_index
                        .seek_point(&project(&u, lhs_field, &mut bytes_scanned))
                        .filter_map(|ptr| rhs_table.get_row_ref(blob_store, ptr))
                        .map(Row::Ptr)
                        .map(Tuple::Row)
                    {
                        f(v)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: false,
                semijoin: Semi::All,
                ..
            } => {
                // Probe the index and evaluate the matching rhs rows
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    for v in rhs_index
                        .seek_point(&project(&u, lhs_field, &mut bytes_scanned))
                        .filter_map(|ptr| rhs_table.get_row_ref(blob_store, ptr))
                        .map(Row::Ptr)
                        .map(Tuple::Row)
                    {
                        f(u.clone().join(v.clone()))?;
                    }
                    Ok(())
                })?;
            }
        }
        metrics.index_seeks += index_seeks;
        metrics.rows_scanned += n;
        metrics.bytes_scanned += bytes_scanned;
        Ok(())
    }
}

/// An index join executor where the index (rhs) side is a delta table.
///
/// TODO: There is much overlap between this executor and [PipelinedIxJoin].
/// But merging them requires merging the [Datastore] and [DeltaStore] traits,
/// since the index scan interface is right now split between both.
#[derive(Debug)]
pub struct PipelinedIxDeltaJoin {
    /// The executor for the lhs of the join
    pub lhs: Box<PipelinedExecutor>,
    /// The rhs table
    pub rhs_table: TableId,
    /// Inserts or deletes?
    pub rhs_delta: Delta,
    /// The rhs index
    pub rhs_index: IndexId,
    /// The rhs join field
    pub rhs_field: ColId,
    /// The lhs join field
    pub lhs_field: TupleField,
    /// Is the index unique?
    pub unique: bool,
    /// Is this a semijoin?
    pub semijoin: Semi,
}

impl PipelinedIxDeltaJoin {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        match self.rhs_delta {
            Delta::Inserts => !tx.has_inserts(self.rhs_table) || self.lhs.is_empty(tx),
            Delta::Deletes => !tx.has_deletes(self.rhs_table) || self.lhs.is_empty(tx),
        }
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut index_seeks = 0;
        let mut bytes_scanned = 0;

        match self {
            Self {
                lhs,
                lhs_field,
                unique: true,
                semijoin: Semi::Lhs,
                ..
            } => {
                // Should we evaluate the lhs tuple?
                // Probe the index to see if there is a matching row.
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if tx
                        .index_scan_point_for_delta(
                            self.rhs_table,
                            self.rhs_index,
                            self.rhs_delta,
                            &project(&u, lhs_field, &mut bytes_scanned),
                        )
                        .next()
                        .is_some()
                    {
                        f(u)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: true,
                semijoin: Semi::Rhs,
                ..
            } => {
                // Probe the index and evaluate the matching rhs row
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(v) = tx
                        .index_scan_point_for_delta(
                            self.rhs_table,
                            self.rhs_index,
                            self.rhs_delta,
                            &project(&u, lhs_field, &mut bytes_scanned),
                        )
                        .next()
                        .map(Tuple::Row)
                    {
                        f(v)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: true,
                semijoin: Semi::All,
                ..
            } => {
                // Probe the index and evaluate the matching rhs row
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(v) = tx
                        .index_scan_point_for_delta(
                            self.rhs_table,
                            self.rhs_index,
                            self.rhs_delta,
                            &project(&u, lhs_field, &mut bytes_scanned),
                        )
                        .next()
                        .map(Tuple::Row)
                    {
                        f(u.join(v))?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: false,
                semijoin: Semi::Lhs,
                ..
            } => {
                // How many times should we evaluate the lhs tuple?
                // Probe the index for the number of matching rows.
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    for _ in 0..tx
                        .index_scan_point_for_delta(
                            self.rhs_table,
                            self.rhs_index,
                            self.rhs_delta,
                            &project(&u, lhs_field, &mut bytes_scanned),
                        )
                        .count()
                    {
                        f(u.clone())?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: false,
                semijoin: Semi::Rhs,
                ..
            } => {
                // Probe the index and evaluate the matching rhs rows
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    for v in tx
                        .index_scan_point_for_delta(
                            self.rhs_table,
                            self.rhs_index,
                            self.rhs_delta,
                            &project(&u, lhs_field, &mut bytes_scanned),
                        )
                        .map(Tuple::Row)
                    {
                        f(v)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                lhs_field,
                unique: false,
                semijoin: Semi::All,
                ..
            } => {
                // Probe the index and evaluate the matching rhs rows
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    for v in tx
                        .index_scan_point_for_delta(
                            self.rhs_table,
                            self.rhs_index,
                            self.rhs_delta,
                            &project(&u, lhs_field, &mut bytes_scanned),
                        )
                        .map(Tuple::Row)
                    {
                        f(u.clone().join(v.clone()))?;
                    }
                    Ok(())
                })?;
            }
        }
        metrics.index_seeks += index_seeks;
        metrics.rows_scanned += n;
        metrics.bytes_scanned += bytes_scanned;
        Ok(())
    }
}

/// An executor for a hash join.
/// Note, this executor is a pipeline breaker,
/// because it must fully materialize the rhs.
#[derive(Debug)]
pub struct BlockingHashJoin {
    pub lhs: Box<PipelinedExecutor>,
    pub rhs: Box<PipelinedExecutor>,
    pub lhs_field: TupleField,
    pub rhs_field: TupleField,
    pub unique: bool,
    pub semijoin: Semi,
}

impl BlockingHashJoin {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.lhs.is_empty(tx) || self.rhs.is_empty(tx)
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut bytes_scanned = 0;
        match self {
            Self {
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: true,
                semijoin: Semi::Lhs,
            } => {
                let mut rhs_table = HashSet::new();
                rhs.execute(tx, metrics, &mut |v| {
                    rhs_table.insert(project(&v, rhs_field, &mut bytes_scanned));
                    Ok(())
                })?;

                // How many rows did we pull from the rhs?
                n += rhs_table.len();

                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if rhs_table.contains(&project(&u, lhs_field, &mut bytes_scanned)) {
                        f(u)?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: true,
                semijoin: Semi::Rhs,
            } => {
                let mut rhs_table = HashMap::new();
                rhs.execute(tx, metrics, &mut |v| {
                    rhs_table.insert(project(&v, rhs_field, &mut bytes_scanned), v);
                    Ok(())
                })?;

                // How many rows did we pull from the rhs?
                n += rhs_table.len();

                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(v) = rhs_table.get(&project(&u, lhs_field, &mut bytes_scanned)) {
                        f(v.clone())?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: true,
                semijoin: Semi::All,
            } => {
                let mut rhs_table = HashMap::new();
                rhs.execute(tx, metrics, &mut |v| {
                    rhs_table.insert(project(&v, rhs_field, &mut bytes_scanned), v);
                    Ok(())
                })?;

                // How many rows did we pull from the rhs?
                n += rhs_table.len();

                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(v) = rhs_table.get(&project(&u, lhs_field, &mut bytes_scanned)) {
                        f(u.clone().join(v.clone()))?;
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: false,
                semijoin: Semi::Lhs,
            } => {
                let mut rhs_table = HashMap::new();
                rhs.execute(tx, metrics, &mut |v| {
                    n += 1;
                    rhs_table
                        .entry(project(&v, rhs_field, &mut bytes_scanned))
                        .and_modify(|n| *n += 1)
                        .or_insert(1);
                    Ok(())
                })?;
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(n) = rhs_table.get(&project(&u, lhs_field, &mut bytes_scanned)).copied() {
                        for _ in 0..n {
                            f(u.clone())?;
                        }
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: false,
                semijoin: Semi::Rhs,
            } => {
                let mut rhs_table: HashMap<AlgebraicValue, Vec<_>> = HashMap::new();
                rhs.execute(tx, metrics, &mut |v| {
                    n += 1;
                    let key = project(&v, rhs_field, &mut bytes_scanned);
                    if let Some(tuples) = rhs_table.get_mut(&key) {
                        tuples.push(v);
                    } else {
                        rhs_table.insert(key, vec![v]);
                    }
                    Ok(())
                })?;
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(rhs_tuples) = rhs_table.get(&project(&u, lhs_field, &mut bytes_scanned)) {
                        for v in rhs_tuples {
                            f(v.clone())?;
                        }
                    }
                    Ok(())
                })?;
            }
            Self {
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: false,
                semijoin: Semi::All,
            } => {
                let mut rhs_table: HashMap<AlgebraicValue, Vec<_>> = HashMap::new();
                rhs.execute(tx, metrics, &mut |v| {
                    n += 1;
                    let key = project(&v, rhs_field, &mut bytes_scanned);
                    if let Some(tuples) = rhs_table.get_mut(&key) {
                        tuples.push(v);
                    } else {
                        rhs_table.insert(key, vec![v]);
                    }
                    Ok(())
                })?;
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(rhs_tuples) = rhs_table.get(&project(&u, lhs_field, &mut bytes_scanned)) {
                        for v in rhs_tuples {
                            f(u.clone().join(v.clone()))?;
                        }
                    }
                    Ok(())
                })?;
            }
        }
        metrics.rows_scanned += n;
        metrics.bytes_scanned += bytes_scanned;
        Ok(())
    }
}

/// An executor for a nested loop join.
/// Note, this is a pipeline breaker,
/// because it fully materializes the rhs.
#[derive(Debug)]
pub struct BlockingNLJoin {
    pub lhs: Box<PipelinedExecutor>,
    pub rhs: Box<PipelinedExecutor>,
}

impl BlockingNLJoin {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.lhs.is_empty(tx) || self.rhs.is_empty(tx)
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut rhs = vec![];
        self.rhs.execute(tx, metrics, &mut |v| {
            rhs.push(v);
            Ok(())
        })?;

        // How many rows did we pull from the rhs?
        let mut n = rhs.len();

        self.lhs.execute(tx, metrics, &mut |u| {
            n += 1;
            for v in rhs.iter() {
                f(u.clone().join(v.clone()))?;
            }
            Ok(())
        })?;

        metrics.rows_scanned += n;
        Ok(())
    }
}

/// A pipelined filter executor
#[derive(Debug)]
pub struct PipelinedFilter {
    pub input: Box<PipelinedExecutor>,
    pub expr: PhysicalExpr,
}

impl PipelinedFilter {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.input.is_empty(tx)
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut bytes_scanned = 0;
        self.input.execute(tx, metrics, &mut |t| {
            n += 1;
            if self.expr.eval_bool_with_metrics(&t, &mut bytes_scanned) {
                f(t)?;
            }
            Ok(())
        })?;
        metrics.rows_scanned += n;
        metrics.bytes_scanned += bytes_scanned;
        Ok(())
    }
}

/// A pipelined limit operator that does not short-circuit.
/// Input rows will be scanned even after the limit has been reached.
#[derive(Debug)]
pub struct PipelinedLimit {
    pub input: Box<PipelinedExecutor>,
    pub limit: u64,
}

impl PipelinedLimit {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.input.is_empty(tx)
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        self.input.execute(tx, metrics, &mut |t| {
            n += 1;
            if n <= self.limit as usize {
                f(t)?;
            }
            Ok(())
        })?;
        metrics.rows_scanned += n;
        Ok(())
    }
}

/// A wrapper around [ProjectField] that increments a counter by the size of the projected value
fn project(row: &impl ProjectField, field: &TupleField, bytes_scanned: &mut usize) -> AlgebraicValue {
    let value = row.project(field);
    *bytes_scanned += value.size_of();
    value
}
