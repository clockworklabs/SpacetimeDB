use anyhow::{bail, Context as _, Result};
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashSet};
use spacetimedb_execution::{
    pipelined::{
        PipelinedExecutor, PipelinedIxDeltaJoin, PipelinedIxDeltaScanEq, PipelinedIxDeltaScanRange, PipelinedIxJoin,
        PipelinedIxScanEq, PipelinedIxScanRange, PipelinedProject,
    },
    Datastore, DeltaStore, Row,
};
use spacetimedb_expr::{
    check::SchemaView,
    cypher::translate_cypher,
    expr::{CollectViews, ProjectList},
};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, query::Delta, AlgebraicValue};
use spacetimedb_physical_plan::{
    compile::compile_select,
    plan::{IxJoin, IxScan, Label, PhysicalPlan, ProjectPlan, Sarg, TableScan, TupleField},
};
use spacetimedb_primitives::{ColId, ColList, IndexId, TableId, ViewId};
use spacetimedb_query::compile_subscription;
use spacetimedb_schema::table_name::TableName;
use std::ops::RangeBounds;

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
                PipelinedExecutor::IxScanEq(PipelinedIxScanEq { table_id, index_id, .. })
                | PipelinedExecutor::IxScanRange(PipelinedIxScanRange { table_id, index_id, .. })
                | PipelinedExecutor::IxDeltaScanEq(PipelinedIxDeltaScanEq { table_id, index_id, .. })
                | PipelinedExecutor::IxDeltaScanRange(PipelinedIxDeltaScanRange { table_id, index_id, .. })
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
    /// Build delta fragments for incremental view maintenance.
    ///
    /// `table_ids` and `table_aliases` are parallel slices: `table_ids[i]` is
    /// the underlying `TableId` for the scan labelled `table_aliases[i]`.
    ///
    /// When multiple aliases share the same `TableId` (self-join, e.g. a
    /// graph query scanning `Vertex` twice), they are grouped together so
    /// that a single table mutation sets the delta flag on **all** aliases of
    /// that table simultaneously.  The IVM formula then operates on the
    /// number of *distinct* tables (≤ 2 for subscriptions).
    fn compile_from_plan(
        plan: &ProjectPlan,
        table_ids: &[TableId],
        table_aliases: &[Label],
        auth: &AuthCtx,
    ) -> Result<Self> {
        let groups = group_aliases_by_table(table_ids, table_aliases);

        fn mut_plan_group(plan: &mut ProjectPlan, labels: &[Label], delta: Delta) {
            for &label in labels {
                plan.visit_mut(&mut |plan| match plan {
                    PhysicalPlan::TableScan(
                        scan @ TableScan {
                            limit: None,
                            delta: None,
                            ..
                        },
                        alias,
                    ) if alias == &label => {
                        scan.delta = Some(delta);
                    }
                    _ => {}
                });
            }
        }

        fn new_plan(
            plan: &ProjectPlan,
            specs: &[(&[Label], Delta)],
            auth: &AuthCtx,
        ) -> Result<PipelinedProject> {
            let mut plan = plan.clone();
            for (labels, delta) in specs {
                mut_plan_group(&mut plan, labels, *delta);
            }
            plan.optimize(auth).map(PipelinedProject::from)
        }

        match groups.as_slice() {
            [dr] => Ok(Fragments {
                insert_plans: vec![new_plan(plan, &[(dr, Delta::Inserts)], auth)?],
                delete_plans: vec![new_plan(plan, &[(dr, Delta::Deletes)], auth)?],
            }),
            [dr, ds] => Ok(Fragments {
                insert_plans: vec![
                    new_plan(plan, &[(dr, Delta::Inserts)], auth)?,
                    new_plan(plan, &[(ds, Delta::Inserts)], auth)?,
                    new_plan(plan, &[(dr, Delta::Inserts), (ds, Delta::Deletes)], auth)?,
                    new_plan(plan, &[(dr, Delta::Deletes), (ds, Delta::Inserts)], auth)?,
                ],
                delete_plans: vec![
                    new_plan(plan, &[(dr, Delta::Deletes)], auth)?,
                    new_plan(plan, &[(ds, Delta::Deletes)], auth)?,
                    new_plan(plan, &[(dr, Delta::Inserts), (ds, Delta::Inserts)], auth)?,
                    new_plan(plan, &[(dr, Delta::Deletes), (ds, Delta::Deletes)], auth)?,
                ],
            }),
            _ => bail!(
                "Subscriptions support at most 2 distinct tables, found {}",
                groups.len()
            ),
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

    /// Does this plan return rows from an event table?
    pub fn returns_event_table(&self) -> bool {
        self.plan_opt.return_table().is_some_and(|schema| schema.is_event)
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

        let mut subscriptions = vec![];

        for plan in plans {
            let plan_opt = plan.clone().optimize(auth)?;

            if has_non_index_join(&plan_opt) {
                bail!("Subscriptions require indexes on join columns")
            }

            if plan_opt.reads_from_event_table() {
                bail!("Event tables cannot be used as the lookup table in subscription joins")
            }

            let (table_ids, table_aliases) = table_ids_for_plan(&plan);

            let fragments = Fragments::compile_from_plan(&plan, &table_ids, &table_aliases, auth)?;

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

    /// Generate subscription plans from a Cypher graph query.
    ///
    /// Fixed-depth graph queries (e.g. `MATCH (a)-[r]->(b) RETURN a`)
    /// lower to `EqJoin` chains over `Vertex` and `Edge` tables. These
    /// are compiled into subscription fragments using the standard 2-table
    /// incremental view maintenance formula, with aliases grouped by their
    /// underlying `TableId` so that self-joins (same table scanned under
    /// multiple aliases) share deltas correctly.
    ///
    /// Variable-length path queries (`[*1..k]`) are expanded into a UNION
    /// of fixed-depth plans — one `SubscriptionPlan` per depth.
    pub fn compile_cypher(
        cypher: &str,
        tx: &impl SchemaView,
        auth: &AuthCtx,
    ) -> Result<Vec<Self>> {
        let query = spacetimedb_cypher_parser::parse_cypher(cypher)
            .context("Failed to parse Cypher query")?;

        let project_list = translate_cypher(&query, tx)
            .context("Failed to translate Cypher query")?;

        let names = match project_list {
            ProjectList::Name(names) => names,
            _ => bail!("Graph subscriptions must return whole table rows (RETURN * or RETURN <var>)"),
        };

        if names.is_empty() {
            bail!("Empty Cypher query result");
        }

        let return_id = names
            .first()
            .and_then(|n| n.return_table_id())
            .ok_or_else(|| anyhow::anyhow!("Cannot determine return TableId for Cypher subscription"))?;

        debug_assert!(
            names.iter().all(|n| n.return_table_id() == Some(return_id)),
            "All depth levels must share the same return table"
        );

        let return_name = tx
            .schema_for_table(return_id)
            .map(|s| s.table_name.clone())
            .ok_or_else(|| anyhow::anyhow!("TableId `{return_id}` does not exist"))?;

        let mut subscriptions = vec![];

        for name in names {
            let plan = compile_select(name);
            let plan_opt = plan.clone().optimize(auth)?;

            if has_non_index_join(&plan_opt) {
                bail!("Graph subscriptions require indexes on join columns")
            }

            if plan_opt.reads_from_event_table() {
                bail!("Event tables cannot be used as the lookup table in graph subscription joins")
            }

            let (table_ids, table_aliases) = table_ids_for_plan(&plan);

            let fragments = Fragments::compile_from_plan(&plan, &table_ids, &table_aliases, auth)?;

            subscriptions.push(Self {
                return_id,
                return_name: return_name.clone(),
                table_ids,
                plan_opt,
                fragments,
            });
        }

        Ok(subscriptions)
    }
}

/// Does this plan have any non-index joins?
fn has_non_index_join(plan: &PhysicalPlan) -> bool {
    plan.any(&|op| matches!(op, PhysicalPlan::HashJoin(..) | PhysicalPlan::NLJoin(..)))
}

/// Collect `(TableId, Label)` pairs from all scans in a physical plan.
fn table_ids_for_plan(plan: &PhysicalPlan) -> (Vec<TableId>, Vec<Label>) {
    let mut table_aliases = vec![];
    let mut table_ids = vec![];
    plan.visit(&mut |plan| match plan {
        PhysicalPlan::TableScan(TableScan { schema, .. }, alias)
        | PhysicalPlan::IxScan(IxScan { schema, .. }, alias) => {
            table_aliases.push(*alias);
            table_ids.push(schema.table_id);
        }
        _ => {}
    });
    (table_ids, table_aliases)
}

/// Group labels by their underlying `TableId`.
///
/// Returns a `Vec<Vec<Label>>` where each inner vec contains all labels
/// that scan the same physical table. This allows the IVM formula to
/// treat self-joins correctly: when a table mutates, ALL aliases of that
/// table see the same delta.
fn group_aliases_by_table(table_ids: &[TableId], table_aliases: &[Label]) -> Vec<Vec<Label>> {
    let mut groups: Vec<Vec<Label>> = Vec::new();
    let mut seen: Vec<TableId> = Vec::new();

    for (&tid, &alias) in table_ids.iter().zip(table_aliases.iter()) {
        if let Some(pos) = seen.iter().position(|&id| id == tid) {
            groups[pos].push(alias);
        } else {
            seen.push(tid);
            groups.push(vec![alias]);
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_expr::check::test_utils::SchemaViewer;
    use spacetimedb_lib::{
        db::raw_def::v9::{btree, RawModuleDefV9Builder},
        identity::AuthCtx,
        AlgebraicType, ProductType,
    };
    use spacetimedb_primitives::TableId;

    fn graph_tx() -> SchemaViewer {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                "Vertex",
                ProductType::from([
                    ("Id", AlgebraicType::U64),
                    ("Label", AlgebraicType::String),
                    ("Properties", AlgebraicType::String),
                ]),
                true,
            )
            .with_index_no_accessor_name(btree(ColId(0)));
        builder
            .build_table_with_new_type(
                "Edge",
                ProductType::from([
                    ("Id", AlgebraicType::U64),
                    ("StartId", AlgebraicType::U64),
                    ("EndId", AlgebraicType::U64),
                    ("EdgeType", AlgebraicType::String),
                    ("Properties", AlgebraicType::String),
                ]),
                true,
            )
            .with_index_no_accessor_name(btree(ColId(1)))
            .with_index_no_accessor_name(btree(ColId(2)));
        let module_def = builder.finish().try_into().expect("valid module def");
        SchemaViewer::new(module_def, vec![("Vertex", TableId(0)), ("Edge", TableId(1))])
    }

    #[test]
    fn group_aliases_single_table() {
        let ids = vec![TableId(0)];
        let labels = vec![Label(1)];
        let groups = group_aliases_by_table(&ids, &labels);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![Label(1)]);
    }

    #[test]
    fn group_aliases_two_distinct_tables() {
        let ids = vec![TableId(0), TableId(1)];
        let labels = vec![Label(1), Label(2)];
        let groups = group_aliases_by_table(&ids, &labels);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![Label(1)]);
        assert_eq!(groups[1], vec![Label(2)]);
    }

    #[test]
    fn group_aliases_self_join() {
        let ids = vec![TableId(0), TableId(1), TableId(0)];
        let labels = vec![Label(1), Label(2), Label(3)];
        let groups = group_aliases_by_table(&ids, &labels);
        assert_eq!(groups.len(), 2, "3 aliases over 2 tables should produce 2 groups");
        assert_eq!(groups[0], vec![Label(1), Label(3)], "both Vertex aliases grouped");
        assert_eq!(groups[1], vec![Label(2)], "Edge alias in its own group");
    }

    #[test]
    fn compile_cypher_single_hop() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b) RETURN a",
            &tx,
            &auth,
        );
        let plans = result.expect("single-hop graph subscription should compile");
        assert_eq!(plans.len(), 1, "fixed-depth single-hop produces 1 plan");

        let plan = &plans[0];
        assert!(plan.is_join(), "graph query is a join subscription");
        assert_eq!(
            plan.fragments.insert_plans.len(),
            4,
            "2-table IVM produces 4 insert fragments"
        );
        assert_eq!(
            plan.fragments.delete_plans.len(),
            4,
            "2-table IVM produces 4 delete fragments"
        );
    }

    #[test]
    fn compile_cypher_two_hop() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b)-[s]->(c) RETURN a",
            &tx,
            &auth,
        );
        let plans = result.expect("two-hop graph subscription should compile");
        assert_eq!(plans.len(), 1, "fixed-depth two-hop produces 1 plan");
        assert!(plans[0].is_join());
    }

    #[test]
    fn compile_cypher_return_star_single_node() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let plans = SubscriptionPlan::compile_cypher("MATCH (a) RETURN *", &tx, &auth)
            .expect("RETURN * on single node should compile");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].subscribed_table_id(), TableId(0));
    }

    #[test]
    fn compile_cypher_return_star_join_rejected() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b) RETURN *",
            &tx,
            &auth,
        );
        assert!(
            result.is_err(),
            "RETURN * on a join should fail (ambiguous return table)"
        );
    }

    #[test]
    fn compile_cypher_variable_length_produces_union() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[*1..3]->(b) RETURN a",
            &tx,
            &auth,
        );
        let plans = result.expect("variable-length subscription should compile");
        assert_eq!(
            plans.len(),
            3,
            "[*1..3] expands to 3 fixed-depth plans (depths 1, 2, 3)"
        );
        for plan in &plans {
            assert_eq!(plan.fragments.insert_plans.len(), 4);
            assert_eq!(plan.fragments.delete_plans.len(), 4);
        }
    }

    #[test]
    fn compile_cypher_with_label_filter() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a",
            &tx,
            &auth,
        );
        let plans = result.expect("labelled graph subscription should compile");
        assert_eq!(plans.len(), 1);
        assert!(plans[0].is_join());
    }

    #[test]
    fn compile_cypher_subscribed_table_is_vertex() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let plans = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b) RETURN a",
            &tx,
            &auth,
        )
        .unwrap();
        assert_eq!(
            plans[0].subscribed_table_id(),
            TableId(0),
            "RETURN a should subscribe to Vertex (TableId 0)"
        );
    }

    #[test]
    fn compile_cypher_rejects_property_projection() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b) RETURN a.Label",
            &tx,
            &auth,
        );
        assert!(
            result.is_err(),
            "property projections should be rejected for subscriptions"
        );
    }

    fn graph_tx_no_indexes() -> SchemaViewer {
        use spacetimedb_expr::check::test_utils::build_module_def;
        SchemaViewer::new(
            build_module_def(vec![
                (
                    "Vertex",
                    ProductType::from([
                        ("Id", AlgebraicType::U64),
                        ("Label", AlgebraicType::String),
                        ("Properties", AlgebraicType::String),
                    ]),
                ),
                (
                    "Edge",
                    ProductType::from([
                        ("Id", AlgebraicType::U64),
                        ("StartId", AlgebraicType::U64),
                        ("EndId", AlgebraicType::U64),
                        ("EdgeType", AlgebraicType::String),
                        ("Properties", AlgebraicType::String),
                    ]),
                ),
            ]),
            vec![("Vertex", TableId(0)), ("Edge", TableId(1))],
        )
    }

    #[test]
    fn compile_cypher_rejects_non_index_join() {
        let tx = graph_tx_no_indexes();
        let auth = AuthCtx::for_testing();
        let result = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b) RETURN a",
            &tx,
            &auth,
        );
        assert!(
            result.is_err(),
            "graph joins without indexes should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("indexes on join columns"),
            "error should mention missing indexes, got: {err}"
        );
    }

    #[test]
    fn compile_cypher_single_hop_table_ids() {
        let tx = graph_tx();
        let auth = AuthCtx::for_testing();
        let plans = SubscriptionPlan::compile_cypher(
            "MATCH (a)-[r]->(b) RETURN a",
            &tx,
            &auth,
        )
        .unwrap();

        let plan = &plans[0];
        let tids: Vec<TableId> = plan.table_ids().collect();
        assert!(
            tids.contains(&TableId(0)),
            "should read from Vertex table"
        );
        assert!(
            tids.contains(&TableId(1)),
            "should read from Edge table"
        );
    }
}
