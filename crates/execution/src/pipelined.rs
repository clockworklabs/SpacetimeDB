use std::{
    collections::{HashMap, HashSet},
    ops::Bound,
};

use anyhow::Result;
use itertools::Either;
use spacetimedb_expr::expr::AggType;
use spacetimedb_lib::{metrics::ExecutionMetrics, query::Delta, sats::size_of::SizeOf, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::plan::{
    HashJoin, IndexProbe, IxJoin, IxScan, PhysicalExpr, PhysicalPlan, ProjectField, ProjectListPlan, ProjectPlan, Semi,
    TableScan, TupleField,
};
use spacetimedb_primitives::{ColList, IndexId, TableId};
use spacetimedb_sats::product;

use crate::{Datastore, DeltaStore, Row, Tuple};

/// An executor for explicit column projections.
/// Note, this plan can only be constructed from the http api,
/// which is not considered performance critical.
/// Hence this operator is not particularly optimized.
pub enum ProjectListExecutor {
    Name(Vec<PipelinedProject>),
    View(Vec<ViewProject>),
    List(Vec<PipelinedExecutor>, Vec<TupleField>),
    Limit(Box<ProjectListExecutor>, u64),
    Agg(Vec<PipelinedExecutor>, AggType),
}

impl From<ProjectListPlan> for ProjectListExecutor {
    fn from(plan: ProjectListPlan) -> Self {
        /// A helper that checks if a [`ProjectListPlan`] returns an unprojected view table
        fn returns_view_table(plans: &[ProjectPlan]) -> bool {
            plans.first().is_some_and(|plan| plan.returns_view_table())
        }

        /// A helper that returns the number of columns returned by this [`ProjectListPlan`]
        fn num_cols(plans: &[ProjectPlan]) -> usize {
            plans
                .first()
                .and_then(|plan| plan.return_table())
                .map(|schema| schema.num_cols())
                .unwrap_or_default()
        }

        /// A helper that returns the number of private columns returned by this [`ProjectListPlan`]
        fn num_private_cols(plans: &[ProjectPlan]) -> usize {
            plans
                .first()
                .and_then(|plan| plan.return_table())
                .map(|schema| schema.num_private_cols())
                .unwrap_or_default()
        }

        match plan {
            ProjectListPlan::Name(plans) if returns_view_table(&plans) => {
                let num_cols = num_cols(&plans);
                let num_private_cols = num_private_cols(&plans);
                Self::View(
                    plans
                        .into_iter()
                        .map(PipelinedProject::from)
                        .map(|plan| ViewProject::new(plan, num_cols, num_private_cols))
                        .collect(),
                )
            }
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
            Self::View(plans) => {
                for plan in plans {
                    plan.execute(tx, metrics, &mut |row| {
                        n += 1;
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
                            n += tx.row_count(table_scan.table) as usize;
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

/// An executor for a query that returns rows from a view.
/// Essentially just a projection that drops the view's private columns.
///
/// Unlike user tables, view tables can have private columns.
/// For example, if a view is not anonymous, its backing table will have a `sender` column.
/// This column tracks which rows belong to which caller of the view.
/// However we must remove this column before sending rows from the view to a client.
///
/// See `TableSchema::from_view_def_for_datastore` for more details.
#[derive(Debug)]
pub struct ViewProject {
    num_cols: usize,
    num_private_cols: usize,
    inner: PipelinedProject,
}

impl ViewProject {
    pub fn new(inner: PipelinedProject, num_cols: usize, num_private_cols: usize) -> Self {
        Self {
            inner,
            num_cols,
            num_private_cols,
        }
    }

    pub fn execute<Tx: Datastore + DeltaStore>(
        &self,
        tx: &Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(ProductValue) -> Result<()>,
    ) -> Result<()> {
        let mut n = 0;
        let mut bytes_scanned = 0;
        self.inner.execute(tx, metrics, &mut |row| match row {
            Row::Ptr(ptr) => {
                n += 1;
                let col_list = ColList::from_iter(self.num_private_cols..self.num_cols);
                let row = ptr.project_product(&col_list)?;
                bytes_scanned += row.size_of();
                f(row)
            }
            Row::Ref(val) => {
                n += 1;
                let col_list = ColList::from_iter(self.num_private_cols..self.num_cols);
                let row = val.project_product(&col_list)?;
                bytes_scanned += row.size_of();
                f(row)
            }
        })?;
        metrics.rows_scanned += n;
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
            PhysicalPlan::IxScan(scan, _) => Self::IxScan(scan.into()),
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_index,
                    unique,
                    probe,
                    rhs_delta,
                    ..
                },
                semijoin,
            ) => Self::IxJoin(PipelinedIxJoin {
                lhs: Box::new(Self::from(*lhs)),
                rhs_table: rhs.table_id,
                rhs_index,
                source: IndexSource::from_delta(rhs_delta),
                probe,
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
            | Self::Filter(PipelinedFilter { input, .. })
            | Self::Limit(PipelinedLimit { input, .. }) => {
                input.visit(f);
            }
            Self::NLJoin(BlockingNLJoin { lhs, rhs }) | Self::HashJoin(BlockingHashJoin { lhs, rhs, .. }) => {
                lhs.visit(f);
                rhs.visit(f);
            }
            Self::TableScan(..) | Self::IxScan(..) => {}
        }
    }

    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        match self {
            Self::TableScan(scan) => scan.is_empty(tx),
            Self::IxScan(scan) => scan.is_empty(tx),
            Self::IxJoin(join) => join.is_empty(tx),
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
            Self::IxJoin(join) => join.execute(tx, metrics, f),
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

#[derive(Debug, Clone, Copy)]
enum IndexSource {
    Base,
    Delta(Delta),
}

impl IndexSource {
    fn from_delta(delta: Option<Delta>) -> Self {
        match delta {
            None => Self::Base,
            Some(delta) => Self::Delta(delta),
        }
    }

    fn is_empty(self, tx: &impl DeltaStore, table_id: TableId) -> bool {
        match self {
            Self::Base => false,
            Self::Delta(Delta::Inserts) => !tx.has_inserts(table_id),
            Self::Delta(Delta::Deletes) => !tx.has_deletes(table_id),
        }
    }
}

#[derive(Debug)]
enum EvaluatedIndexProbe {
    Point(AlgebraicValue),
    Range(Bound<AlgebraicValue>, Bound<AlgebraicValue>),
}

impl From<IndexProbe> for EvaluatedIndexProbe {
    fn from(probe: IndexProbe) -> Self {
        match probe {
            IndexProbe::Point(point) => Self::Point(eval_static_probe(point)),
            IndexProbe::Range(lower, upper) => Self::Range(eval_static_bound(lower), eval_static_bound(upper)),
        }
    }
}

/// A pipelined executor for scanning an index.
#[derive(Debug)]
pub struct PipelinedIxScan {
    /// The table id.
    pub table_id: TableId,
    /// The index id.
    pub index_id: IndexId,
    pub limit: Option<u64>,
    source: IndexSource,
    probe: EvaluatedIndexProbe,
}

impl From<IxScan> for PipelinedIxScan {
    fn from(scan: IxScan) -> Self {
        let IxScan {
            schema,
            index_id,
            limit,
            delta,
            probe,
        } = scan;
        Self {
            table_id: schema.table_id,
            index_id,
            limit,
            source: IndexSource::from_delta(delta),
            probe: probe.into(),
        }
    }
}

struct NoRow;

impl ProjectField for NoRow {
    fn project(&self, _: &TupleField) -> AlgebraicValue {
        panic!("field expression in standalone index scan probe")
    }
}

fn eval_static_probe(expr: PhysicalExpr) -> AlgebraicValue {
    expr.eval_with_metrics(&NoRow, &mut 0).into_owned()
}

fn eval_static_bound(bound: Bound<PhysicalExpr>) -> Bound<AlgebraicValue> {
    match bound {
        Bound::Included(expr) => Bound::Included(eval_static_probe(expr)),
        Bound::Excluded(expr) => Bound::Excluded(eval_static_probe(expr)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

fn eval_probe(expr: &PhysicalExpr, row: &impl ProjectField, bytes_scanned: &mut usize) -> AlgebraicValue {
    expr.eval_with_metrics(row, bytes_scanned).into_owned()
}

fn for_each_index_scan_row<'a, Tx: Datastore + DeltaStore>(
    tx: &'a Tx,
    source: IndexSource,
    table_id: TableId,
    index_id: IndexId,
    probe: &EvaluatedIndexProbe,
    limit: Option<u64>,
    f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
) -> Result<usize> {
    // Datastore and DeltaStore expose different index iterator item types, so
    // normalize both into Tuple at this boundary while keeping the traits split.
    let mut n = 0;
    let mut emit = |tuple| {
        n += 1;
        f(tuple)
    };

    match (source, probe) {
        (IndexSource::Base, EvaluatedIndexProbe::Point(point)) => {
            let scan = tx.index_scan_point(table_id, index_id, point)?;
            match limit {
                None => {
                    for row in scan {
                        emit(Tuple::Row(Row::Ptr(row)))?;
                    }
                }
                Some(limit) => {
                    for row in scan.take(limit as usize) {
                        emit(Tuple::Row(Row::Ptr(row)))?;
                    }
                }
            }
        }
        (IndexSource::Base, EvaluatedIndexProbe::Range(lower, upper)) => {
            let scan = tx.index_scan_range(table_id, index_id, &(lower.as_ref(), upper.as_ref()))?;
            match limit {
                None => {
                    for row in scan {
                        emit(Tuple::Row(Row::Ptr(row)))?;
                    }
                }
                Some(limit) => {
                    for row in scan.take(limit as usize) {
                        emit(Tuple::Row(Row::Ptr(row)))?;
                    }
                }
            }
        }
        (IndexSource::Delta(delta), EvaluatedIndexProbe::Point(point)) => {
            // Delta index scans did not apply IxScan::limit before this refactor.
            for row in tx.index_scan_point_for_delta(table_id, index_id, delta, point) {
                emit(Tuple::Row(row))?;
            }
        }
        (IndexSource::Delta(delta), EvaluatedIndexProbe::Range(lower, upper)) => {
            // Delta index scans did not apply IxScan::limit before this refactor.
            for row in tx.index_scan_range_for_delta(table_id, index_id, delta, (lower.as_ref(), upper.as_ref())) {
                emit(Tuple::Row(row))?;
            }
        }
    }

    Ok(n)
}

fn for_each_index_point<'a, Tx: Datastore + DeltaStore>(
    tx: &'a Tx,
    source: IndexSource,
    table_id: TableId,
    index_id: IndexId,
    point: &AlgebraicValue,
    f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
) -> Result<()> {
    match source {
        IndexSource::Base => {
            for row in tx.index_scan_point(table_id, index_id, point)? {
                f(Tuple::Row(Row::Ptr(row)))?;
            }
        }
        IndexSource::Delta(delta) => {
            for row in tx.index_scan_point_for_delta(table_id, index_id, delta, point) {
                f(Tuple::Row(row))?;
            }
        }
    }
    Ok(())
}

fn first_index_point<'a, Tx: Datastore + DeltaStore>(
    tx: &'a Tx,
    source: IndexSource,
    table_id: TableId,
    index_id: IndexId,
    point: &AlgebraicValue,
) -> Result<Option<Tuple<'a>>> {
    Ok(match source {
        IndexSource::Base => tx
            .index_scan_point(table_id, index_id, point)?
            .next()
            .map(Row::Ptr)
            .map(Tuple::Row),
        IndexSource::Delta(delta) => tx
            .index_scan_point_for_delta(table_id, index_id, delta, point)
            .next()
            .map(Tuple::Row),
    })
}

impl PipelinedIxScan {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.source.is_empty(tx, self.table_id)
    }

    pub fn execute<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Tuple<'a>) -> Result<()>,
    ) -> Result<()> {
        let n = for_each_index_scan_row(
            tx,
            self.source,
            self.table_id,
            self.index_id,
            &self.probe,
            self.limit,
            f,
        )?;
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
    source: IndexSource,
    /// The point probe evaluated against each lhs tuple.
    pub probe: PhysicalExpr,
    /// Is the index unique?
    pub unique: bool,
    /// Is this a semijoin?
    pub semijoin: Semi,
}

impl PipelinedIxJoin {
    /// Does this operation contain an empty delta scan?
    pub fn is_empty(&self, tx: &impl DeltaStore) -> bool {
        self.lhs.is_empty(tx) || self.source.is_empty(tx, self.rhs_table)
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

        let iter_rhs =
            |u: &Tuple, bytes_scanned: &mut usize, f: &mut dyn FnMut(Tuple<'a>) -> Result<()>| -> Result<()> {
                let key = eval_probe(&self.probe, u, bytes_scanned);
                for_each_index_point(tx, self.source, self.rhs_table, self.rhs_index, &key, f)
            };

        let probe_rhs = |u: &Tuple, bytes_scanned: &mut usize| -> Result<_> {
            let key = eval_probe(&self.probe, u, bytes_scanned);
            first_index_point(tx, self.source, self.rhs_table, self.rhs_index, &key)
        };

        match (self.unique, self.semijoin) {
            (true, Semi::Lhs) => {
                // Should we evaluate the lhs tuple?
                // Probe the index to see if there is a matching row.
                self.lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if probe_rhs(&u, &mut bytes_scanned)?.is_some() {
                        f(u)?;
                    }
                    Ok(())
                })?;
            }
            (true, Semi::Rhs) => {
                // Probe the index and evaluate the matching rhs row
                self.lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(v) = probe_rhs(&u, &mut bytes_scanned)? {
                        f(v)?;
                    }
                    Ok(())
                })?;
            }
            (true, Semi::All) => {
                // Probe the index and evaluate the matching rhs row
                self.lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    if let Some(v) = probe_rhs(&u, &mut bytes_scanned)? {
                        f(u.join(v))?;
                    }
                    Ok(())
                })?;
            }
            (false, Semi::Lhs) => {
                // How many times should we evaluate the lhs tuple?
                // Probe the index for the number of matching rows.
                self.lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    iter_rhs(&u, &mut bytes_scanned, &mut |_| f(u.clone()))?;
                    Ok(())
                })?;
            }
            (false, Semi::Rhs) => {
                // Probe the index and evaluate the matching rhs rows
                self.lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    iter_rhs(&u, &mut bytes_scanned, f)?;
                    Ok(())
                })?;
            }
            (false, Semi::All) => {
                // Probe the index and evaluate the matching rhs rows
                self.lhs.execute(tx, metrics, &mut |u| {
                    n += 1;
                    index_seeks += 1;
                    iter_rhs(&u, &mut bytes_scanned, &mut |v| f(u.clone().join(v)))?;
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
