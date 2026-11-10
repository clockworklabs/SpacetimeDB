use anyhow::{bail, Result};
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashSet};
use spacetimedb_execution::{
    bindings::BindingId,
    pipelined::{BindingProject, PipelinedExecutor, PipelinedIxJoin, PipelinedIxScan, PipelinedProject},
    Datastore, DeltaStore, ExecutionParams, Row,
};
use spacetimedb_expr::{
    check::SchemaView,
    expr::{CollectViews, ViewCall},
};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, query::Delta, AlgebraicType, AlgebraicValue};
use spacetimedb_physical_plan::plan::{
    IxScan, Label, ParamResolver, ParamSlot, PhysicalExpr, PhysicalPlan, ProjectPlan, TableScan,
};
use spacetimedb_primitives::{ColList, IndexId, TableId};
use spacetimedb_query::{compile_subscription, SubscriptionTemplate};
use spacetimedb_schema::{schema::TableSchema, table_name::TableName};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

/// A subscription is a view over a particular table.
/// How do we incrementally maintain that view?
/// These are the query fragments that are required.
/// See [Self::compile_from_plan] for how to generate them.
#[derive(Debug)]
struct Fragments {
    /// Plan fragments that return rows to insert, each with the binding it matched.
    /// For joins there will be 4 fragments,
    /// but for selects only one.
    insert_plans: Vec<BindingProject>,
    /// Plan fragments that return rows to delete, each with the binding it matched.
    /// For joins there will be 4 fragments,
    /// but for selects only one.
    delete_plans: Vec<BindingProject>,
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
    /// and evaluate a closure over each one and the binding it matched.
    fn for_each_insert<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(BindingId, Row<'a>) -> Result<()>,
    ) -> Result<()> {
        for plan in &self.insert_plans {
            if !plan.is_empty(tx) {
                plan.execute(tx, &NoParams, metrics, f)?;
            }
        }
        Ok(())
    }

    /// A subscription is just a view of a particular table.
    /// Here we compute the rows that are to be removed from that view,
    /// and evaluate a closure over each one and the binding it matched.
    fn for_each_delete<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(BindingId, Row<'a>) -> Result<()>,
    ) -> Result<()> {
        for plan in &self.delete_plans {
            if !plan.is_empty(tx) {
                plan.execute(tx, &NoParams, metrics, f)?;
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
    ///
    /// Each fragment looks up the bindings its rows match, through the binding indexes in `binding_indexes`.
    fn compile_from_plan(plan: &ProjectPlan, tables: &[Label], binding_indexes: &mut Vec<ColList>) -> Result<Self> {
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

        // Return a new plan with delta scans for the given tables
        let mut new_plan = |plan: &ProjectPlan, tables: &[(Label, Delta)]| -> Result<BindingProject> {
            let mut plan = plan.clone();
            for (alias, delta) in tables {
                mut_plan(&mut plan, *alias, *delta);
            }
            let plan = plan.insert_binding_lookup(binding_indexes).optimize()?;
            Ok(BindingProject::new(plan))
        };

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
        PhysicalExpr::Value(_) | PhysicalExpr::Param(..) => {}
    }
}

/// Metadata cached with a subscription after query planning is complete.
#[derive(Debug)]
struct SubscriptionMetadata {
    /// A subscription can read from multiple tables.
    table_ids: Vec<TableId>,
    /// The table or view returned by this plan, if it returns whole rows.
    return_schema: Option<Arc<TableSchema>>,
    /// View calls read by this plan.
    view_calls: Vec<ViewCall>,
    /// Whether this plan reads from an anonymous view.
    reads_anonymous_view: bool,
    /// Whether this plan reads from a non-anonymous view.
    reads_non_anonymous_view: bool,
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
    /// Runtime bindings for this per-sender cached plan.
    params: ExecutionParams,
    /// Metadata derived from the physical plan at compile time.
    metadata: SubscriptionMetadata,
}

impl CollectViews for SubscriptionPlan {
    fn collect_views(&self, views: &mut HashSet<ViewCall>) {
        views.extend(self.metadata.view_calls.iter().cloned());
    }
}

impl SubscriptionPlan {
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

    pub fn params(&self) -> &ExecutionParams {
        &self.params
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

    /// Scan classification used for runtime metrics.
    pub fn scan_metrics(&self) -> &SubscriptionPlanMetrics {
        &self.metadata.scan_metrics
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

        let mut subscriptions = vec![];
        let mut physical_plans = vec![];
        let params = ExecutionParams::from_auth(auth);

        for plan in plans {
            let plan_opt = plan.clone().optimize()?;

            if has_non_index_join(&plan_opt) {
                bail!("Subscriptions require indexes on join columns")
            }

            if plan_opt.reads_from_event_table() {
                bail!("Event tables cannot be used as the lookup table in subscription joins")
            }

            let (table_ids, _) = table_ids_for_plan(&plan);

            // Collect from the plan before optimization, which still has every view call's args.
            let mut view_calls = HashSet::new();
            plan.collect_views(&mut view_calls);

            let metadata = SubscriptionMetadata {
                table_ids,
                return_schema: plan_opt.return_table(),
                view_calls: view_calls.into_iter().collect(),
                reads_anonymous_view: plan_opt.reads_from_view(true),
                reads_non_anonymous_view: plan_opt.reads_from_view(false),
                scan_metrics: SubscriptionPlanMetrics::from_physical_plan(plan_opt.physical_plan()),
            };

            physical_plans.push(plan_opt.clone());

            subscriptions.push(Self {
                return_id,
                return_name: return_name.clone(),
                base_plan: PipelinedProject::from(plan_opt),
                params,
                metadata,
            });
        }

        Ok((subscriptions, has_param, physical_plans))
    }
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

/// A subscription whose plans read every literal and built-in from a binding,
/// shared by the queries that differ only in those values
#[derive(Debug)]
pub struct Template {
    /// The plans before optimization, one per RLS fragment, which identify the template
    key: Vec<ProjectPlan>,
    /// The hash of `key`, which equal keys share
    key_hash: u64,
    /// To which table are we subscribed?
    return_id: TableId,
    /// The name of the table we're subscribed to
    return_name: TableName,
    /// The type of each binding value, in slot order
    param_types: Vec<AlgebraicType>,
    /// The columns of each binding index, which the delta plans probe
    binding_indexes: Vec<ColList>,
    /// One per RLS fragment
    fragments: Vec<TemplateFragment>,
}

/// One RLS fragment of a [`Template`]
#[derive(Debug)]
pub struct TemplateFragment {
    /// The tables this fragment reads
    table_ids: Vec<TableId>,
    /// The table or view returned by this fragment, if it returns whole rows
    return_schema: Option<Arc<TableSchema>>,
    /// The plan fragments for updating the view
    fragments: Fragments,
}

impl Template {
    /// Builds the delta plans of `compiled`, which look up the bindings each row matches
    pub fn build(compiled: &SubscriptionTemplate) -> Result<Self> {
        let mut binding_indexes = vec![];
        let fragments = compiled
            .plans
            .iter()
            .map(|plan| {
                let (table_ids, table_aliases) = table_ids_for_plan(plan);
                Ok(TemplateFragment {
                    table_ids,
                    return_schema: plan.return_table(),
                    fragments: Fragments::compile_from_plan(plan, &table_aliases, &mut binding_indexes)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            key: compiled.plans.clone(),
            key_hash: Self::hash_key(&compiled.plans),
            return_id: compiled.return_id,
            return_name: compiled.return_name.clone(),
            param_types: compiled.params.iter().map(|(_, ty)| ty.clone()).collect(),
            binding_indexes,
            fragments,
        })
    }

    /// The plans before optimization, one per RLS fragment, which identify the template
    pub fn key(&self) -> &[ProjectPlan] {
        &self.key
    }

    /// The hash of the template's key, which equal keys share
    pub fn key_hash(&self) -> u64 {
        self.key_hash
    }

    /// The hash of a template's key, from its plans before optimization
    pub fn hash_key(plans: &[ProjectPlan]) -> u64 {
        let mut state = DefaultHasher::new();
        plans.len().hash(&mut state);
        for plan in plans {
            plan.hash_structure(&mut state);
        }
        state.finish()
    }

    /// To which table are we subscribed?
    pub fn subscribed_table_id(&self) -> TableId {
        self.return_id
    }

    /// To which table are we subscribed?
    pub fn subscribed_table_name(&self) -> &TableName {
        &self.return_name
    }

    /// The type of each binding value, in slot order
    pub fn param_types(&self) -> &[AlgebraicType] {
        &self.param_types
    }

    /// The columns of each binding index, which the delta plans probe
    pub fn binding_indexes(&self) -> &[ColList] {
        &self.binding_indexes
    }

    /// The plan fragments, one per RLS fragment
    pub fn fragments(&self) -> &[TemplateFragment] {
        &self.fragments
    }

    /// From which tables does this template read?
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> + use<> {
        self.fragments
            .iter()
            .flat_map(|fragment| fragment.table_ids.iter().copied())
            .collect::<HashSet<_>>()
            .into_iter()
    }

    /// Returns the index ids from which the delta plans read
    pub fn index_ids(&self) -> impl Iterator<Item = (TableId, IndexId)> + use<> {
        self.fragments
            .iter()
            .flat_map(|fragment| fragment.fragments.index_ids())
            .collect::<HashSet<_>>()
            .into_iter()
    }
}

impl TemplateFragment {
    /// From which tables does this fragment read?
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> + '_ {
        self.table_ids.iter().copied()
    }

    /// Does this fragment return rows from a view?
    pub fn is_view(&self) -> bool {
        self.return_schema.as_ref().is_some_and(|schema| schema.is_view())
    }

    /// Does this fragment return rows from an event table?
    pub fn returns_event_table(&self) -> bool {
        self.return_schema.as_ref().is_some_and(|schema| schema.is_event)
    }

    /// The number of columns returned.
    /// Only relevant if [`Self::is_view`] is true.
    pub fn num_cols(&self) -> usize {
        self.return_schema
            .as_ref()
            .map(|schema| schema.num_cols())
            .unwrap_or_default()
    }

    /// The number of private columns returned.
    /// Only relevant if [`Self::is_view`] is true.
    pub fn num_private_cols(&self) -> usize {
        self.return_schema
            .as_ref()
            .map(|schema| schema.num_private_cols())
            .unwrap_or_default()
    }

    /// Is this a fragment of a join?
    pub fn is_join(&self) -> bool {
        self.fragments.insert_plans.len() > 1 && self.fragments.delete_plans.len() > 1
    }

    /// The rows to insert into the view, each with the binding it matched
    pub fn for_each_insert<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(BindingId, Row<'a>) -> Result<()>,
    ) -> Result<()> {
        self.fragments.for_each_insert(tx, metrics, f)
    }

    /// The rows to delete from the view, each with the binding it matched
    pub fn for_each_delete<'a, Tx: Datastore + DeltaStore>(
        &self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
        f: &mut dyn FnMut(BindingId, Row<'a>) -> Result<()>,
    ) -> Result<()> {
        self.fragments.for_each_delete(tx, metrics, f)
    }
}

/// The params of a template's delta plans, which have none: their lookup reads the binding row instead
struct NoParams;

impl ParamResolver for NoParams {
    fn resolve_param(&self, param: ParamSlot, _: &AlgebraicType) -> AlgebraicValue {
        unreachable!("a template's delta plans have no params: {param:?}")
    }
}
