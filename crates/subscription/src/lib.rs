use anyhow::{bail, Result};
use spacetimedb_execution::{
    pipelined::{
        PipelinedExecutor, PipelinedIxDeltaJoin, PipelinedIxDeltaScan, PipelinedIxJoin, PipelinedIxScan,
        PipelinedProject,
    },
    Datastore, DeltaStore, Row,
};
use spacetimedb_expr::{check::SchemaView, expr::CollectViews};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, query::Delta, AlgebraicValue};
use spacetimedb_physical_plan::plan::{IxJoin, IxScan, Label, PhysicalPlan, ProjectPlan, Sarg, TableScan, TupleField};
use spacetimedb_primitives::{ColId, ColList, IndexId, TableId, ViewId};
use spacetimedb_query::compile_subscription;
use std::sync::Arc;
use std::{collections::HashSet, ops::RangeBounds};

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
                | PipelinedExecutor::IxDeltaScan(PipelinedIxDeltaScan { table_id, index_id, .. })
                | PipelinedExecutor::IxJoin(PipelinedIxJoin {
                    rhs_table: table_id,
                    rhs_index: index_id,
                    ..
                })
                | PipelinedExecutor::IxDeltaJoin(PipelinedIxDeltaJoin {
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

/// Newtype wrapper for table names.
///
/// Uses an `Arc` internally, so `Clone` is cheap.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName(Arc<str>);

impl From<Arc<str>> for TableName {
    fn from(name: Arc<str>) -> Self {
        TableName(name)
    }
}

impl From<Box<str>> for TableName {
    fn from(name: Box<str>) -> Self {
        TableName(name.into())
    }
}

impl From<String> for TableName {
    fn from(name: String) -> Self {
        TableName(name.into())
    }
}

impl std::ops::Deref for TableName {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TableName {
    pub fn table_name_from_str(name: &str) -> Self {
        TableName(name.into())
    }
}

impl std::fmt::Display for TableName {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
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
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
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

/// A subscription defines a view over a table
#[derive(Debug)]
pub struct SubscriptionPlan {
    /// To which table are we subscribed?
    return_id: TableId,
    /// To which table are we subscribed?
    return_name: TableName,
    /// A subscription can read from multiple tables.
    /// From which tables do we read?
    table_ids: Vec<TableId>,
    /// The plan fragments for updating the view
    fragments: Fragments,
    /// The optimized plan without any delta scans
    plan_opt: ProjectPlan,
}

impl CollectViews for SubscriptionPlan {
    fn collect_views(&self, views: &mut HashSet<ViewId>) {
        self.plan_opt.collect_views(views);
    }
}

impl SubscriptionPlan {
    /// Is this a plan for a join?
    pub fn is_join(&self) -> bool {
        self.fragments.insert_plans.len() > 1 && self.fragments.delete_plans.len() > 1
    }

    /// Does this plan return rows from a view?
    pub fn is_view(&self) -> bool {
        self.plan_opt.returns_view_table()
    }

    /// The number of columns returned.
    /// Only relevant if [`Self::is_view`] is true.
    pub fn num_cols(&self) -> usize {
        self.plan_opt
            .return_table()
            .map(|schema| schema.num_cols())
            .unwrap_or_default()
    }

    /// The number of private columns returned.
    /// Only relevant if [`Self::is_view`] is true.
    pub fn num_private_cols(&self) -> usize {
        self.plan_opt
            .return_table()
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
        self.table_ids.iter().copied()
    }

    /// The optimized plan without any delta scans
    pub fn optimized_physical_plan(&self) -> &ProjectPlan {
        &self.plan_opt
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
        if !self.is_join() {
            return None;
        }
        let mut join_edge = None;
        self.plan_opt.visit(&mut |op| match op {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: lhs_join_col,
                    lhs_field:
                        TupleField {
                            field_pos: rhs_join_col,
                            ..
                        },
                    ..
                },
                _,
            ) if rhs.table_id == self.return_id => match &**lhs {
                PhysicalPlan::IxScan(
                    IxScan {
                        schema,
                        prefix,
                        arg: Sarg::Eq(rhs_col, rhs_val),
                        ..
                    },
                    _,
                ) if schema.table_id != self.return_id
                    && prefix.is_empty()
                    && schema.is_unique(&ColList::new((*rhs_join_col).into())) =>
                {
                    let lhs_table = self.return_id;
                    let rhs_table = schema.table_id;
                    let rhs_col = *rhs_col;
                    let rhs_val = rhs_val.clone();
                    let lhs_join_col = *lhs_join_col;
                    let rhs_join_col = (*rhs_join_col).into();
                    let edge = JoinEdge {
                        lhs_table,
                        rhs_table,
                        lhs_join_col,
                        rhs_join_col,
                        rhs_col,
                    };
                    join_edge = Some((edge, rhs_val));
                }
                _ => {}
            },
            _ => {}
        });
        join_edge
    }

    /// Generate a plan for incrementally maintaining a subscription
    pub fn compile(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<(Vec<Self>, bool)> {
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

        let return_name = TableName::from(return_name);

        for plan in plans {
            let plan_opt = plan.clone().optimize(auth)?;

            if has_non_index_join(&plan_opt) {
                bail!("Subscriptions require indexes on join columns")
            }

            let (table_ids, table_aliases) = table_ids_for_plan(&plan);

            let fragments = Fragments::compile_from_plan(&plan, &table_aliases, auth)?;

            subscriptions.push(Self {
                return_id,
                return_name: return_name.clone(),
                table_ids,
                plan_opt,
                fragments,
            });
        }

        Ok((subscriptions, has_param))
    }
}
