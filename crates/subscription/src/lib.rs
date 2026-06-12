use anyhow::{bail, Result};
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashSet};
use spacetimedb_execution::{
    pipelined::{PipelinedExecutor, PipelinedIxJoin, PipelinedIxScan, PipelinedProject},
    Datastore, DeltaStore, Row,
};
use spacetimedb_expr::{check::SchemaView, expr::CollectViews};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, query::Delta, AlgebraicValue};
use spacetimedb_physical_plan::plan::{IxScan, Label, PhysicalExpr, PhysicalPlan, ProjectPlan, TableScan};
use spacetimedb_primitives::{ColId, ColList, IndexId, TableId, ViewId};
use spacetimedb_query::compile_subscription;
use spacetimedb_schema::{schema::TableSchema, table_name::TableName};
use std::{ops::RangeBounds, sync::Arc};

/// A subscription is a view over a particular table.
/// How do we incrementally maintain that view?
/// These are the query fragments that are required.
/// See [Self::compile_from_plan] for how to generate them.
#[derive(Debug)]
struct Fragments {
    /// Plan fragments that return rows to insert.
    /// For joins there will be 4 fragments,
    /// but for selects only one.
    insert_plans: Vec<PipelinedProject>,
    /// Plan fragments that return rows to delete.
    /// For joins there will be 4 fragments,
    /// but for selects only one.
    delete_plans: Vec<PipelinedProject>,
}

impl Fragments {
    /// Returns the index ids from which this fragment reads.
    fn index_ids(&self) -> impl Iterator<Item = (TableId, IndexId)> + use<> {
        let mut index_ids = HashSet::new();
        for plan in self.insert_plans.iter().chain(self.delete_plans.iter()) {
            plan.visit(&mut |plan| match plan {
                PipelinedExecutor::IxScan(PipelinedIxScan { table_id, index_id, .. })
                | PipelinedExecutor::IxJoin(PipelinedIxJoin {
                    rhs_table: table_id,
                    rhs_index: index_id,
                    ..
                }) => {
                    index_ids.insert((*table_id, *index_id));
                }
                _ => {}
            });
        }
        index_ids.into_iter()
    }

    /// A subscription is just a view of a particular table.
    /// Here we compute the rows that are to be inserted into that view,
    /// and evaluate a closure over each one.
    fn for_each_insert<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Row<'a>) -> Result<()>,
    ) -> Result<()> {
        for plan in &self.insert_plans {
            if !plan.is_empty(tx) {
                plan.execute(tx, metrics, f)?;
            }
        }
        Ok(())
    }

    /// A subscription is just a view of a particular table.
    /// Here we compute the rows that are to be removed from that view,
    /// and evaluate a closure over each one.
    fn for_each_delete<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Row<'a>) -> Result<()>,
    ) -> Result<()> {
        for plan in &self.delete_plans {
            if !plan.is_empty(tx) {
                plan.execute(tx, metrics, f)?;
            }
        }
        Ok(())
    }

    /// Which fragments are required for incrementally updating a subscription?
    /// This is most interesting in the case of a join.
    ///
    /// Let `V`  denote the join between tables `R` and `S` at time `t`.
    /// Let `V'` denote the same join at time `t+1`.
    ///
    /// We then have the following equality
    ///
    /// ```text
    /// V' = V U dv
    /// ```
    ///
    /// where `dv` is called the delta of `V`.
    ///
    /// So how do we compute `dv` incrementally?
    /// That is, without evaluating `R' x S'`.
    /// and without access to the state at time `t`.
    ///
    /// Given the following notation:
    ///
    /// ```text
    /// x: The relational join operator
    /// U: union
    /// -: difference
    ///
    /// dv: The difference or delta between V and V'
    ///
    /// dv(+): Rows in V' that are not in V
    /// dv(-): Rows in V  that are not in V'
    /// ```
    ///
    /// we derive the following equations
    ///
    /// ```text
    /// V  = R x S
    ///    = RS
    ///
    /// V' = V  U dv
    ///    = RS U dv
    ///
    /// V' = R' x S'
    ///    = (R U dr) x (S U ds)
    ///    = RS U Rds U drS U drds
    ///
    /// dv = Rds U drS U drds
    ///    = (R' - dr)ds U dr(S' - ds) U drds
    ///    = R'ds - drds U drS' - drds U drds
    ///    = R'ds U drS' - drds
    ///    = R'(ds(+) - ds(-)) U (dr(+) - dr(-))S' - (dr(+) - dr(-))(ds(+) - ds(-))
    ///    = R'ds(+)
    ///         - R'ds(-)
    ///         U dr(+)S'
    ///         - dr(-)S'
    ///         - dr(+)ds(+)
    ///         U dr(+)ds(-)
    ///         U dr(-)ds(+)
    ///         - dr(-)ds(-)
    ///    = R'ds(+)
    ///         U dr(+)S'
    ///         U dr(+)ds(-)
    ///         U dr(-)ds(+)
    ///         - R'ds(-)
    ///         - dr(-)S'
    ///         - dr(+)ds(+)
    ///         - dr(-)ds(-)
    ///
    /// dv(+) = R'ds(+) U dr(+)S' U dr(+)ds(-) U dr(-)ds(+)
    /// dv(-) = R'ds(-) U dr(-)S' U dr(+)ds(+) U dr(-)ds(-)
    /// ```
    fn compile_from_plan(plan: &ProjectPlan, tables: &[Label], auth: &AuthCtx) -> Result<Self> {
        /// Mutate a query plan by turning a table scan into a delta scan
        fn mut_plan(plan: &mut ProjectPlan, relvar: Label, delta: Delta) {
            plan.visit_mut(&mut |plan| match plan {
                PhysicalPlan::TableScan(
                    scan @ TableScan {
                        limit: None,
                        delta: None,
                        ..
                    },
                    alias,
                ) if alias == &relvar => {
                    scan.delta = Some(delta);
                }
                _ => {}
            });
        }

        /// Return a new plan with delta scans for the given tables
        fn new_plan(plan: &ProjectPlan, tables: &[(Label, Delta)], auth: &AuthCtx) -> Result<PipelinedProject> {
            let mut plan = plan.clone();
            for (alias, delta) in tables {
                mut_plan(&mut plan, *alias, *delta);
            }
            plan.optimize(auth).map(PipelinedProject::from)
        }

        match tables {
            [dr] => Ok(Fragments {
                insert_plans: vec![new_plan(plan, &[(*dr, Delta::Inserts)], auth)?],
                delete_plans: vec![new_plan(plan, &[(*dr, Delta::Deletes)], auth)?],
            }),
            [dr, ds] => Ok(Fragments {
                insert_plans: vec![
                    new_plan(
                        // dr(+)S'
                        plan,
                        &[(*dr, Delta::Inserts)],
                        auth,
                    )?,
                    new_plan(
                        // R'ds(+)
                        plan,
                        &[(*ds, Delta::Inserts)],
                        auth,
                    )?,
                    new_plan(
                        // dr(+)ds(-)
                        plan,
                        &[(*dr, Delta::Inserts), (*ds, Delta::Deletes)],
                        auth,
                    )?,
                    new_plan(
                        // dr(-)ds(+)
                        plan,
                        &[(*dr, Delta::Deletes), (*ds, Delta::Inserts)],
                        auth,
                    )?,
                ],
                delete_plans: vec![
                    new_plan(
                        // dr(-)S'
                        plan,
                        &[(*dr, Delta::Deletes)],
                        auth,
                    )?,
                    new_plan(
                        // R'ds(-)
                        plan,
                        &[(*ds, Delta::Deletes)],
                        auth,
                    )?,
                    new_plan(
                        // dr(+)ds(+)
                        plan,
                        &[(*dr, Delta::Inserts), (*ds, Delta::Inserts)],
                        auth,
                    )?,
                    new_plan(
                        // dr(-)ds(-)
                        plan,
                        &[(*dr, Delta::Deletes), (*ds, Delta::Deletes)],
                        auth,
                    )?,
                ],
            }),
            _ => bail!("Invalid number of tables in subscription: {}", tables.len()),
        }
    }
}

/// A join edge is used for pruning queries when evaluating subscription updates.
///
/// If we have the following subscriptions:
/// ```sql
/// SELECT a.* FROM a JOIN b ON a.id = b.id WHERE b.x = 1
/// SELECT a.* FROM a JOIN b ON a.id = b.id WHERE b.x = 2
/// ...
/// SELECT a.* FROM a JOIN b ON a.id = b.id WHERE b.x = n
/// ```
///
/// Whenever `a` is updated, only the relevant queries are evaluated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct JoinEdge {
    /// The [`TableId`] for `a`
    pub lhs_table: TableId,
    /// The [`TableId`] for `b`
    pub rhs_table: TableId,
    /// The [`ColId`] for `a.id`
    pub lhs_join_col: ColId,
    /// The [`ColId`] for `b.id`
    pub rhs_join_col: ColId,
    /// The [`ColId`] for `b.x`
    pub rhs_col: ColId,
}

impl JoinEdge {
    /// A helper method for finding a range of join edges for a particular table in a sorted set.
    fn min_for_table(lhs_table: TableId) -> Self {
        Self {
            lhs_table,
            rhs_table: TableId(u32::MIN),
            lhs_join_col: ColId(u16::MIN),
            rhs_join_col: ColId(u16::MIN),
            rhs_col: ColId(u16::MIN),
        }
    }

    /// A helper method for finding a range of join edges for a particular table in a sorted set.
    fn max_for_table(lhs_table: TableId) -> Self {
        Self {
            lhs_table,
            rhs_table: TableId(u32::MAX),
            lhs_join_col: ColId(u16::MAX),
            rhs_join_col: ColId(u16::MAX),
            rhs_col: ColId(u16::MAX),
        }
    }

    /// A helper method for finding a range of join edges for a particular table in a sorted set.
    pub fn range_for_table(lhs_table: TableId) -> impl RangeBounds<Self> {
        Self::min_for_table(lhs_table)..=Self::max_for_table(lhs_table)
    }
}

/// Metrics metadata derived once when a subscription plan is compiled.
#[derive(Debug, Clone)]
pub struct SubscriptionPlanMetrics {
    scan_type: String,
    unindexed_columns: String,
}

impl SubscriptionPlanMetrics {
    fn from_physical_plan(plan: &PhysicalPlan) -> Self {
        let has_table_scan = plan.any(&|p| matches!(p, PhysicalPlan::TableScan(..)));
        let has_index_scan = plan.any(&|p| matches!(p, PhysicalPlan::IxScan(..)));
        let has_post_filter = plan.any(&|p| matches!(p, PhysicalPlan::Filter(..)));

        let scan_type = if has_table_scan && has_index_scan {
            "mixed"
        } else if has_table_scan {
            "sequential"
        } else if has_index_scan && has_post_filter {
            "indexed_with_filter"
        } else if has_index_scan {
            "fully_indexed"
        } else {
            "unknown"
        }
        .to_owned();

        let mut schema: Option<Arc<TableSchema>> = None;
        plan.visit(&mut |p| match p {
            PhysicalPlan::TableScan(scan, _) => {
                schema = Some(scan.schema.clone());
            }
            PhysicalPlan::IxScan(scan, _) => {
                schema = Some(scan.schema.clone());
            }
            _ => {}
        });

        let mut columns = Vec::new();
        plan.visit(&mut |p| {
            if let PhysicalPlan::Filter(_, expr) = p {
                extract_columns(expr, schema.as_ref(), &mut columns);
            }
        });

        Self {
            scan_type,
            unindexed_columns: columns.join(","),
        }
    }

    pub fn scan_type(&self) -> &str {
        &self.scan_type
    }

    pub fn unindexed_columns(&self) -> &str {
        &self.unindexed_columns
    }
}

fn extract_columns(expr: &PhysicalExpr, schema: Option<&Arc<TableSchema>>, columns: &mut Vec<String>) {
    match expr {
        PhysicalExpr::Field(tuple_field) => {
            let col_name = schema
                .and_then(|s| s.columns.get(tuple_field.field_pos))
                .map(|col| col.col_name.to_string())
                .unwrap_or_else(|| format!("col_{}", tuple_field.field_pos));
            columns.push(col_name);
        }
        PhysicalExpr::BinOp(_, lhs, rhs) => {
            extract_columns(lhs, schema, columns);
            extract_columns(rhs, schema, columns);
        }
        PhysicalExpr::LogOp(_, exprs) => {
            for expr in exprs {
                extract_columns(expr, schema, columns);
            }
        }
        PhysicalExpr::Product(exprs) => {
            for expr in exprs {
                extract_columns(expr, schema, columns);
            }
        }
        PhysicalExpr::Value(_) => {}
    }
}

/// Metadata cached with a subscription after query planning is complete.
#[derive(Debug)]
struct SubscriptionMetadata {
    /// A subscription can read from multiple tables.
    table_ids: Vec<TableId>,
    /// The table or view returned by this plan, if it returns whole rows.
    return_schema: Option<Arc<TableSchema>>,
    /// View ids read by this plan.
    view_ids: Vec<ViewId>,
    /// Whether this plan reads from an anonymous view.
    reads_anonymous_view: bool,
    /// Whether this plan reads from a non-anonymous view.
    reads_non_anonymous_view: bool,
    /// Search arguments used for pruning.
    search_args: Vec<(TableId, ColId, AlgebraicValue)>,
    /// Join edge used for pruning.
    join_edge: Option<(JoinEdge, AlgebraicValue)>,
    /// Scan classification used for runtime metrics.
    scan_metrics: SubscriptionPlanMetrics,
}

/// A subscription defines a view over a table
#[derive(Debug)]
pub struct SubscriptionPlan {
    /// To which table are we subscribed?
    return_id: TableId,
    /// To which table are we subscribed?
    return_name: TableName,
    /// The cached executor for the non-incremental query plan.
    base_plan: PipelinedProject,
    /// The plan fragments for updating the view
    fragments: Fragments,
    /// Metadata derived from the physical plan at compile time.
    metadata: SubscriptionMetadata,
}

impl CollectViews for SubscriptionPlan {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        views.extend(self.metadata.view_ids.iter().copied());
    }
}

impl SubscriptionPlan {
    /// Is this a plan for a join?
    pub fn is_join(&self) -> bool {
        self.fragments.insert_plans.len() > 1 && self.fragments.delete_plans.len() > 1
    }

    /// Does this plan return rows from a view?
    pub fn is_view(&self) -> bool {
        self.metadata
            .return_schema
            .as_ref()
            .is_some_and(|schema| schema.is_view())
    }

    /// Does this plan return rows from an event table?
    pub fn returns_event_table(&self) -> bool {
        self.metadata
            .return_schema
            .as_ref()
            .is_some_and(|schema| schema.is_event)
    }

    /// The number of columns returned.
    /// Only relevant if [`Self::is_view`] is true.
    pub fn num_cols(&self) -> usize {
        self.metadata
            .return_schema
            .as_ref()
            .map(|schema| schema.num_cols())
            .unwrap_or_default()
    }

    /// The number of private columns returned.
    /// Only relevant if [`Self::is_view`] is true.
    pub fn num_private_cols(&self) -> usize {
        self.metadata
            .return_schema
            .as_ref()
            .map(|schema| schema.num_private_cols())
            .unwrap_or_default()
    }

    /// To which table does this plan subscribe?
    pub fn subscribed_table_id(&self) -> TableId {
        self.return_id
    }

    /// To which table does this plan subscribe?
    pub fn subscribed_table_name(&self) -> &TableName {
        &self.return_name
    }

    /// From which tables does this plan read?
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> + '_ {
        self.metadata.table_ids.iter().copied()
    }

    /// The cached executor for the non-incremental query plan.
    pub fn base_plan(&self) -> &PipelinedProject {
        &self.base_plan
    }

    /// The table or view returned by this plan, if it returns whole rows.
    pub fn return_table(&self) -> Option<&Arc<TableSchema>> {
        self.metadata.return_schema.as_ref()
    }

    /// Does this plan read from an (anonymous) view?
    pub fn reads_from_view(&self, anonymous: bool) -> bool {
        if anonymous {
            self.metadata.reads_anonymous_view
        } else {
            self.metadata.reads_non_anonymous_view
        }
    }

    /// Search arguments used for pruning.
    pub fn search_args(&self) -> impl Iterator<Item = (TableId, ColId, AlgebraicValue)> + '_ {
        self.metadata.search_args.iter().cloned()
    }

    /// Scan classification used for runtime metrics.
    pub fn scan_metrics(&self) -> &SubscriptionPlanMetrics {
        &self.metadata.scan_metrics
    }

    /// From which indexes does this plan read?
    pub fn index_ids(&self) -> impl Iterator<Item = (TableId, IndexId)> + use<> {
        self.fragments.index_ids()
    }

    /// A subscription is just a view of a particular table.
    /// Here we compute the rows that are to be inserted into that view,
    /// and evaluate a closure over each one.
    pub fn for_each_insert<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Row<'a>) -> Result<()>,
    ) -> Result<()> {
        self.fragments.for_each_insert(tx, metrics, f)
    }

    /// A subscription is just a view of a particular table.
    /// Here we compute the rows that are to be removed from that view,
    /// and evaluate a closure over each one.
    pub fn for_each_delete<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(Row<'a>) -> Result<()>,
    ) -> Result<()> {
        self.fragments.for_each_delete(tx, metrics, f)
    }

    /// Returns a join edge for this query if it has one.
    ///
    /// Requirements include:
    /// 1. Unique join index
    /// 2. Single column index lookup on the rhs table
    /// 3. No self joins
    pub fn join_edge(&self) -> Option<(JoinEdge, AlgebraicValue)> {
        self.metadata.join_edge.clone()
    }

    fn join_edge_for_plan(
        plan_opt: &ProjectPlan,
        return_id: TableId,
        is_join: bool,
    ) -> Option<(JoinEdge, AlgebraicValue)> {
        if !is_join {
            return None;
        }
        let mut join_edge = None;
        plan_opt.visit(&mut |op| match op {
            PhysicalPlan::IxJoin(join, _) if join.rhs.table_id == return_id => {
                let Some((lhs_join_col, lhs_field)) = join.single_probe_field() else {
                    return;
                };
                let rhs_join_col = ColId(lhs_field.field_pos as u16);
                match &*join.lhs {
                    PhysicalPlan::IxScan(scan, _)
                        if scan.schema.table_id != return_id && scan.schema.is_unique(&ColList::new(rhs_join_col)) =>
                    {
                        let Some((rhs_col, rhs_val)) = scan.single_col_lit_point() else {
                            return;
                        };
                        let edge = JoinEdge {
                            lhs_table: return_id,
                            rhs_table: scan.schema.table_id,
                            lhs_join_col,
                            rhs_join_col,
                            rhs_col,
                        };
                        join_edge = Some((edge, rhs_val.clone()));
                    }
                    _ => {}
                }
            }
            _ => {}
        });
        join_edge
    }

    /// Generate a plan for incrementally maintaining a subscription
    pub fn compile(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<(Vec<Self>, bool)> {
        Self::compile_plans(sql, tx, auth).map(|(plans, has_param, _)| (plans, has_param))
    }

    /// Generate a plan for incrementally maintaining a subscription
    pub fn compile_plans(
        sql: &str,
        tx: &impl SchemaView,
        auth: &AuthCtx,
    ) -> Result<(Vec<Self>, bool, Vec<ProjectPlan>)> {
        let (plans, return_id, return_name, has_param) = compile_subscription(sql, tx, auth)?;

        /// Does this plan have any non-index joins?
        fn has_non_index_join(plan: &PhysicalPlan) -> bool {
            plan.any(&|op| matches!(op, PhysicalPlan::HashJoin(..) | PhysicalPlan::NLJoin(..)))
        }

        /// What tables are involved in this plan?
        fn table_ids_for_plan(plan: &PhysicalPlan) -> (Vec<TableId>, Vec<Label>) {
            let mut table_aliases = vec![];
            let mut table_ids = vec![];
            plan.visit(&mut |plan| match plan {
                PhysicalPlan::TableScan(
                    TableScan {
                        // What table are we reading?
                        schema,
                        ..
                    },
                    alias,
                )
                | PhysicalPlan::IxScan(
                    IxScan {
                        // What table are we reading?
                        schema,
                        ..
                    },
                    alias,
                ) => {
                    table_aliases.push(*alias);
                    table_ids.push(schema.table_id);
                }
                _ => {}
            });
            (table_ids, table_aliases)
        }

        let mut subscriptions = vec![];
        let mut physical_plans = vec![];

        for plan in plans {
            let plan_opt = plan.clone().optimize(auth)?;

            if has_non_index_join(&plan_opt) {
                bail!("Subscriptions require indexes on join columns")
            }

            if plan_opt.reads_from_event_table() {
                bail!("Event tables cannot be used as the lookup table in subscription joins")
            }

            let (table_ids, table_aliases) = table_ids_for_plan(&plan);

            let fragments = Fragments::compile_from_plan(&plan, &table_aliases, auth)?;
            let is_join = fragments.insert_plans.len() > 1 && fragments.delete_plans.len() > 1;

            let mut view_ids = HashSet::new();
            plan_opt.collect_views(&mut view_ids);

            let metadata = SubscriptionMetadata {
                table_ids,
                return_schema: plan_opt.return_table(),
                view_ids: view_ids.into_iter().collect(),
                reads_anonymous_view: plan_opt.reads_from_view(true),
                reads_non_anonymous_view: plan_opt.reads_from_view(false),
                search_args: plan_opt.physical_plan().search_args(),
                join_edge: Self::join_edge_for_plan(&plan_opt, return_id, is_join),
                scan_metrics: SubscriptionPlanMetrics::from_physical_plan(plan_opt.physical_plan()),
            };

            physical_plans.push(plan_opt.clone());

            subscriptions.push(Self {
                return_id,
                return_name: return_name.clone(),
                base_plan: PipelinedProject::from(plan_opt),
                fragments,
                metadata,
            });
        }

        Ok((subscriptions, has_param, physical_plans))
    }
}
