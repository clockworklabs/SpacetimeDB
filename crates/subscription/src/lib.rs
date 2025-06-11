use anyhow::{bail, Result};
use spacetimedb_execution::{
    pipelined::{
        PipelinedExecutor, PipelinedIxDeltaJoin, PipelinedIxDeltaScan, PipelinedIxJoin, PipelinedIxScan,
        PipelinedProject,
    },
    Datastore, DeltaStore, Row,
};
use spacetimedb_expr::check::SchemaView;
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, query::Delta};
use spacetimedb_physical_plan::plan::{HashJoin, IxJoin, Label, PhysicalPlan, ProjectPlan, TableScan};
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_query::compile_subscription;
use std::collections::HashSet;
use std::sync::Arc;

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
    fn index_ids(&self) -> impl Iterator<Item = (TableId, IndexId)> {
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
    fn compile_from_plan(plan: &ProjectPlan, tables: &[Label]) -> Result<Self> {
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
        fn new_plan(plan: &ProjectPlan, tables: &[(Label, Delta)]) -> Result<PipelinedProject> {
            let mut plan = plan.clone();
            for (alias, delta) in tables {
                mut_plan(&mut plan, *alias, *delta);
            }
            plan.optimize().map(PipelinedProject::from)
        }

        match tables {
            [dr] => Ok(Fragments {
                insert_plans: vec![new_plan(plan, &[(*dr, Delta::Inserts)])?],
                delete_plans: vec![new_plan(plan, &[(*dr, Delta::Deletes)])?],
            }),
            [dr, ds] => Ok(Fragments {
                insert_plans: vec![
                    new_plan(
                        // dr(+)S'
                        plan,
                        &[(*dr, Delta::Inserts)],
                    )?,
                    new_plan(
                        // R'ds(+)
                        plan,
                        &[(*ds, Delta::Inserts)],
                    )?,
                    new_plan(
                        // dr(+)ds(-)
                        plan,
                        &[(*dr, Delta::Inserts), (*ds, Delta::Deletes)],
                    )?,
                    new_plan(
                        // dr(-)ds(+)
                        plan,
                        &[(*dr, Delta::Deletes), (*ds, Delta::Inserts)],
                    )?,
                ],
                delete_plans: vec![
                    new_plan(
                        // dr(-)S'
                        plan,
                        &[(*dr, Delta::Deletes)],
                    )?,
                    new_plan(
                        // R'ds(-)
                        plan,
                        &[(*ds, Delta::Deletes)],
                    )?,
                    new_plan(
                        // dr(+)ds(+)
                        plan,
                        &[(*dr, Delta::Inserts), (*ds, Delta::Inserts)],
                    )?,
                    new_plan(
                        // dr(-)ds(-)
                        plan,
                        &[(*dr, Delta::Deletes), (*ds, Delta::Deletes)],
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
    /// The original plan without any delta scans.
    ///
    /// TODO: Used for cardinality estimation,
    /// but not for maintaining the view,
    /// therefore it should ultimately be removed.
    plan: ProjectPlan,
}

impl SubscriptionPlan {
    /// Is this a plan for a join?
    pub fn is_join(&self) -> bool {
        self.fragments.insert_plans.len() > 1 && self.fragments.delete_plans.len() > 1
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

    /// The original plan without any delta scans.
    ///
    /// TODO: Used for cardinality estimation,
    /// but not for maintaining the view,
    /// therefore it should ultimately be removed.
    pub fn physical_plan(&self) -> &ProjectPlan {
        &self.plan
    }

    /// From which indexes does this plan read?
    pub fn index_ids(&self) -> impl Iterator<Item = (TableId, IndexId)> {
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

    /// Generate a plan for incrementally maintaining a subscription
    pub fn compile(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<(Vec<Self>, bool)> {
        let (plans, return_id, return_name, has_param) = compile_subscription(sql, tx, auth)?;

        /// Does this plan have any non-index joins?
        fn has_non_index_join(plan: &PhysicalPlan) -> bool {
            let mut ix_joins = true;
            plan.visit(&mut |plan| match plan {
                PhysicalPlan::IxJoin(IxJoin { lhs_field, .. }, _) => {
                    ix_joins = ix_joins && plan.index_on_field(&lhs_field.label, lhs_field.field_pos);
                }
                PhysicalPlan::HashJoin(
                    HashJoin {
                        lhs_field, rhs_field, ..
                    },
                    _,
                ) => {
                    ix_joins = ix_joins && plan.index_on_field(&lhs_field.label, lhs_field.field_pos);
                    ix_joins = ix_joins && plan.index_on_field(&rhs_field.label, rhs_field.field_pos);
                }
                _ => {}
            });
            !ix_joins
        }

        /// What tables are involved in this plan?
        fn table_ids_for_plan(plan: &PhysicalPlan) -> (Vec<TableId>, Vec<Label>) {
            let mut table_aliases = vec![];
            let mut table_ids = vec![];
            plan.visit(&mut |plan| {
                if let PhysicalPlan::TableScan(
                    TableScan {
                        schema,
                        limit: None,
                        delta: None,
                    },
                    alias,
                ) = plan
                {
                    table_aliases.push(*alias);
                    table_ids.push(schema.table_id);
                }
            });
            (table_ids, table_aliases)
        }

        let mut subscriptions = vec![];

        let return_name = TableName::from(return_name);

        for plan in plans {
            if has_non_index_join(&plan) {
                bail!("Subscriptions require indexes on join columns")
            }

            let (table_ids, table_aliases) = table_ids_for_plan(&plan);

            let fragments = Fragments::compile_from_plan(&plan, &table_aliases)?;

            subscriptions.push(Self {
                return_id,
                return_name: return_name.clone(),
                table_ids,
                plan,
                fragments,
            });
        }

        Ok((subscriptions, has_param))
    }
}
