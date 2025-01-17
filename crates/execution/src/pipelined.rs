use std::{
    collections::{HashMap, HashSet},
    ops::Bound,
};

use anyhow::{anyhow, Result};
use spacetimedb_lib::{query::Delta, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::plan::{
    HashJoin, IxJoin, IxScan, PhysicalExpr, PhysicalPlan, ProjectField, ProjectPlan, Sarg, Semi, TupleField,
};
use spacetimedb_primitives::{ColId, IndexId, TableId};

use crate::{Datastore, DeltaStore, ExecutionMetrics, Row, Tuple};

/// Implements a projection on top of a pipelined executor
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
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
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
        metrics.inc_rows_scanned_by(n);
        Ok(())
    }
}

/// Executes a query plan in a streaming fashion.
/// Avoids materializing intermediate results when possible.
/// Note that unlike a tuple at a time iterator,
/// the caller has no way to interrupt its forward progress.
pub enum PipelinedExecutor {
    TableScan(PipelinedScan),
    IxScan(PipelinedIxScan),
    IxJoin(PipelinedIxJoin),
    HashJoin(BlockingHashJoin),
    NLJoin(BlockingNLJoin),
    Filter(PipelinedFilter),
}

impl From<PhysicalPlan> for PipelinedExecutor {
    fn from(plan: PhysicalPlan) -> Self {
        match plan {
            PhysicalPlan::TableScan(schema, _, delta) => Self::TableScan(PipelinedScan {
                table: schema.table_id,
                delta,
            }),
            PhysicalPlan::IxScan(scan, _) => Self::IxScan(scan.into()),
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_index,
                    rhs_field,
                    unique,
                    lhs_field,
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
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::TableScan(scan) => scan.execute(tx, metrics, f),
            Self::IxScan(scan) => scan.execute(tx, metrics, f),
            Self::IxJoin(join) => join.execute(tx, metrics, f),
            Self::HashJoin(join) => join.execute(tx, metrics, f),
            Self::NLJoin(join) => join.execute(tx, metrics, f),
            Self::Filter(filter) => filter.execute(tx, metrics, f),
        }
    }
}

/// A pipelined executor for scanning both physical and delta tables
pub struct PipelinedScan {
    pub table: TableId,
    pub delta: Option<Delta>,
}

impl PipelinedScan {
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut f = |t| {
            n += 1;
            f(t)
        };
        match self.delta {
            None => {
                for tuple in tx
                    // Open an row id iterator
                    .table_scan(self.table)?
                    .map(Row::Ptr)
                    .map(Tuple::Row)
                {
                    f(tuple)?;
                }
            }
            Some(Delta::Inserts(_)) => {
                for tuple in tx
                    // Open a product value iterator
                    .delta_scan(self.table, true)?
                    .map(Row::Ref)
                    .map(Tuple::Row)
                {
                    f(tuple)?;
                }
            }
            Some(Delta::Deletes(_)) => {
                for tuple in tx
                    // Open a product value iterator
                    .delta_scan(self.table, false)?
                    .map(Row::Ref)
                    .map(Tuple::Row)
                {
                    f(tuple)?;
                }
            }
        }
        metrics.inc_rows_scanned_by(n);
        Ok(())
    }
}

/// A pipelined executor for scanning an index
pub struct PipelinedIxScan {
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
}

impl From<IxScan> for PipelinedIxScan {
    fn from(scan: IxScan) -> Self {
        match scan {
            IxScan {
                schema,
                index_id,
                prefix,
                arg: Sarg::Eq(_, v),
            } => Self {
                table_id: schema.table_id,
                index_id,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower: Bound::Included(v.clone()),
                upper: Bound::Included(v),
            },
            IxScan {
                schema,
                index_id,
                prefix,
                arg: Sarg::Range(_, lower, upper),
            } => Self {
                table_id: schema.table_id,
                index_id,
                prefix: prefix.into_iter().map(|(_, v)| v).collect(),
                lower,
                upper,
            },
        }
    }
}

impl PipelinedIxScan {
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
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
                    .index_scan(
                        self.table_id,
                        self.index_id,
                        &(self.lower.as_ref(), self.upper.as_ref()),
                    )?
                    .map(Row::Ptr)
                    .map(Tuple::Row)
                {
                    f(ptr)?;
                }
            }
            prefix => {
                for ptr in tx
                    .index_scan(
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
                    )?
                    .map(Row::Ptr)
                    .map(Tuple::Row)
                {
                    f(ptr)?;
                }
            }
        }
        metrics.inc_rows_scanned_by(n);
        metrics.inc_index_seeks_by(1);
        Ok(())
    }
}

/// A pipelined index join executor
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
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let blob_store = tx.blob_store();
        let rhs_table = tx.table_or_err(self.rhs_table)?;
        let rhs_index = rhs_table
            .get_index_by_id(self.rhs_index)
            .ok_or_else(|| anyhow!("IndexId `{0}` does not exist", self.rhs_index))?;

        let mut n = 0;
        let mut index_seeks = 0;

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
                    if rhs_index.contains_any(&u.project(lhs_field)) {
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
                        .seek(&u.project(lhs_field))
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
                        .seek(&u.project(lhs_field))
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
                    if let Some(n) = rhs_index.count(&u.project(lhs_field)) {
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
                        .seek(&u.project(lhs_field))
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
                        .seek(&u.project(lhs_field))
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
        metrics.inc_rows_scanned_by(n);
        metrics.inc_index_seeks_by(index_seeks);
        Ok(())
    }
}

/// An executor for a hash join.
/// Note, this executor is a pipeline breaker,
/// because it must fully materialize the rhs.
pub struct BlockingHashJoin {
    pub lhs: Box<PipelinedExecutor>,
    pub rhs: Box<PipelinedExecutor>,
    pub lhs_field: TupleField,
    pub rhs_field: TupleField,
    pub unique: bool,
    pub semijoin: Semi,
}

impl BlockingHashJoin {
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
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
                    rhs_table.insert(v.project(rhs_field));
                    Ok(())
                })?;

                // How many rows did we pull from the rhs?
                n += rhs_table.len();

                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if rhs_table.contains(&u.project(lhs_field)) {
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
                    rhs_table.insert(v.project(rhs_field), v);
                    Ok(())
                })?;

                // How many rows did we pull from the rhs?
                n += rhs_table.len();

                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(v) = rhs_table.get(&u.project(lhs_field)) {
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
                    rhs_table.insert(v.project(rhs_field), v);
                    Ok(())
                })?;

                // How many rows did we pull from the rhs?
                n += rhs_table.len();

                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(v) = rhs_table.get(&u.project(lhs_field)) {
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
                        .entry(v.project(rhs_field))
                        .and_modify(|n| *n += 1)
                        .or_insert(1);
                    Ok(())
                })?;
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(n) = rhs_table.get(&u.project(lhs_field)).copied() {
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
                    let key = v.project(rhs_field);
                    if let Some(tuples) = rhs_table.get_mut(&key) {
                        tuples.push(v);
                    } else {
                        rhs_table.insert(key, vec![v]);
                    }
                    Ok(())
                })?;
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(rhs_tuples) = rhs_table.get(&u.project(lhs_field)) {
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
                    let key = v.project(rhs_field);
                    if let Some(tuples) = rhs_table.get_mut(&key) {
                        tuples.push(v);
                    } else {
                        rhs_table.insert(key, vec![v]);
                    }
                    Ok(())
                })?;
                lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    if let Some(rhs_tuples) = rhs_table.get(&u.project(lhs_field)) {
                        for v in rhs_tuples {
                            f(u.clone().join(v.clone()))?;
                        }
                    }
                    Ok(())
                })?;
            }
        }
        metrics.inc_rows_scanned_by(n);
        Ok(())
    }
}

/// An executor for a nested loop join.
/// Note, this is a pipeline breaker,
/// because it fully materializes the rhs.
pub struct BlockingNLJoin {
    pub lhs: Box<PipelinedExecutor>,
    pub rhs: Box<PipelinedExecutor>,
}

impl BlockingNLJoin {
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut rhs = vec![];
        self.rhs.execute(tx, metrics, &mut |t| {
            rhs.push(t);
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

        metrics.inc_rows_scanned_by(n);
        Ok(())
    }
}

/// A pipelined filter executor
pub struct PipelinedFilter {
    pub input: Box<PipelinedExecutor>,
    pub expr: PhysicalExpr,
}

impl PipelinedFilter {
    pub fn execute<'a, Tx: Datastore + DeltaStore, Metrics: ExecutionMetrics>(
        &self,
        tx: &'a Tx,
        metrics: &mut Metrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        self.input.execute(tx, metrics, &mut |t| {
            n += 1;
            if self.expr.eval_bool(&t) {
                f(t)?;
            }
            Ok(())
        })?;
        metrics.inc_rows_scanned_by(n);
        Ok(())
    }
}
