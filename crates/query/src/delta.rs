use std::ops::Deref;

use anyhow::{bail, Result};
use itertools::Either;
use spacetimedb_execution::{pipelined::PipelinedProject, Datastore, DeltaStore, Row};
use spacetimedb_expr::check::{type_subscription, SchemaView};
use spacetimedb_lib::{metrics::ExecutionMetrics, query::Delta};
use spacetimedb_physical_plan::{
    compile::compile_project_plan,
    plan::{HashJoin, IxJoin, Label, PhysicalPlan, ProjectPlan},
};
use spacetimedb_primitives::TableId;
use spacetimedb_sql_parser::parser::sub::parse_subscription;

use crate::MAX_SQL_LENGTH;

/// A delta plan performs incremental view maintenance
#[derive(Debug)]
pub enum DeltaPlan {
    Join(JoinPlan),
    Select(SelectPlan),
}

impl Deref for DeltaPlan {
    type Target = ProjectPlan;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Join(JoinPlan { plan, .. }) | Self::Select(SelectPlan { plan, .. }) => plan,
        }
    }
}

impl DeltaPlan {
    /// Compile a delta plan for incrementally maintaining a sql view
    pub fn compile(sql: &str, tx: &impl SchemaView) -> Result<Self> {
        if sql.len() > MAX_SQL_LENGTH {
            bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
        }
        let ast = parse_subscription(sql)?;
        let sub = type_subscription(ast, tx)?;

        let Some(table_id) = sub.table_id() else {
            bail!("Failed to determine TableId for query")
        };

        let Some(table_name) = tx.schema_for_table(table_id).map(|schema| schema.table_name.clone()) else {
            bail!("TableId `{table_id}` does not exist")
        };

        let plan = compile_project_plan(sub);

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

        if !ix_joins {
            bail!("Subscriptions require indexes on join columns")
        }

        let mut labels = vec![];
        plan.visit(&mut |plan| {
            if let PhysicalPlan::TableScan(schema, label, None) = plan {
                labels.push((schema.table_id, *label));
            }
        });

        match labels.as_slice() {
            [_] => Ok(Self::Select(SelectPlan {
                table_id,
                table_name,
                plan,
            })),
            [(lhs_table, lhs_label), (rhs_table, rhs_label)] => Ok(Self::Join(JoinPlan {
                table_id,
                table_name,
                lhs_label: *lhs_label,
                rhs_label: *rhs_label,
                lhs_table: *lhs_table,
                rhs_table: *rhs_table,
                plan,
            })),
            _ => bail!("Subscriptions cannot join more than 2 tables"),
        }
    }

    /// Return all tables referenced in the plan
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> {
        match self {
            Self::Select(plan) => Either::Left(plan.table_ids()),
            Self::Join(plan) => Either::Right(plan.table_ids()),
        }
    }

    /// Delta plans always return rows from a single table
    pub fn table_id(&self) -> TableId {
        match self {
            Self::Select(plan) => plan.table_id(),
            Self::Join(plan) => plan.table_id(),
        }
    }

    /// Delta plans always return rows from a single table
    pub fn table_name(&self) -> Box<str> {
        match self {
            Self::Select(plan) => plan.table_name(),
            Self::Join(plan) => plan.table_name(),
        }
    }

    /// Return an evaluator for this delta plan
    pub fn evaluator<Tx: Datastore + DeltaStore>(&self, tx: &Tx) -> DeltaPlanEvaluator {
        match self {
            Self::Select(plan) => plan.evaluator(tx),
            Self::Join(plan) => plan.evaluator(tx),
        }
    }
}

/// An evaluator for a delta plan.
/// It returns the rows that were added to the view,
/// as well as the rows that were removed from it.
pub struct DeltaPlanEvaluator {
    is_join: bool,
    insert_plans: Vec<ProjectPlan>,
    delete_plans: Vec<ProjectPlan>,
}

impl DeltaPlanEvaluator {
    pub fn eval_inserts<'a, Tx: Datastore + DeltaStore>(
        &'a self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
    ) -> Result<impl Iterator<Item = Row<'a>>> {
        let mut rows = vec![];
        for plan in &self.insert_plans {
            let plan = PipelinedProject::from(plan.clone());
            plan.execute(tx, metrics, &mut |row| {
                rows.push(row);
                Ok(())
            })?;
        }
        Ok(rows.into_iter())
    }

    pub fn eval_deletes<'a, Tx: Datastore + DeltaStore>(
        &'a self,
        tx: &'a Tx,
        metrics: &mut ExecutionMetrics,
    ) -> Result<impl Iterator<Item = Row<'a>>> {
        let mut rows = vec![];
        for plan in &self.delete_plans {
            let plan = PipelinedProject::from(plan.clone());
            plan.execute(tx, metrics, &mut |row| {
                rows.push(row);
                Ok(())
            })?;
        }
        Ok(rows.into_iter())
    }

    pub fn is_join(&self) -> bool {
        self.is_join
    }

    pub fn has_inserts(&self) -> bool {
        !self.insert_plans.is_empty()
    }

    pub fn has_deletes(&self) -> bool {
        !self.delete_plans.is_empty()
    }
}

/// A delta plan for a single table select
#[derive(Debug)]
pub struct SelectPlan {
    /// The table whose rows are returned
    table_id: TableId,
    /// The table whose rows are returned
    table_name: Box<str>,
    /// The query plan for the original view
    plan: ProjectPlan,
}

impl SelectPlan {
    /// Delta plans always return rows from a single table
    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    /// Delta plans always return rows from a single table
    pub fn table_name(&self) -> Box<str> {
        self.table_name.clone()
    }

    /// Return all tables referenced in the plan
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> {
        std::iter::once(self.table_id)
    }

    /// Returns an evaluator for computing the view delta
    pub fn evaluator<Tx: Datastore + DeltaStore>(&self, tx: &Tx) -> DeltaPlanEvaluator {
        /// Mutate a query plan by adding delta scans
        fn delta_plan(plan: &ProjectPlan, table_id: TableId, delta: Delta) -> Vec<ProjectPlan> {
            let mut plan = plan.clone();
            plan.visit_mut(&mut |plan| match plan {
                PhysicalPlan::TableScan(schema, _, is_delta @ None) if schema.table_id == table_id => {
                    *is_delta = Some(delta);
                }
                _ => {}
            });
            vec![plan]
        }
        DeltaPlanEvaluator {
            is_join: false,
            insert_plans: tx
                .has_inserts(self.table_id)
                .map(|delta| delta_plan(&self.plan, self.table_id, delta))
                .unwrap_or_default(),
            delete_plans: tx
                .has_deletes(self.table_id)
                .map(|delta| delta_plan(&self.plan, self.table_id, delta))
                .unwrap_or_default(),
        }
    }
}

/// A delta plan for a 2-way join.
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
///         U dr(+)dr(-)
///         - dr(+)ds(+)
///         U dr(-)ds(+)
///         - dr(-)ds(-)
///    = R'ds(+)
///         U dr(+)S'
///         U dr(+)dr(-)
///         U dr(-)ds(+)
///         - R'ds(-)
///         - dr(-)S'
///         - dr(+)ds(+)
///         - dr(-)ds(-)
///
/// dv(+) = R'ds(+) U dr(+)S' U dr(+)dr(-) U dr(-)ds(+)
/// dv(-) = R'ds(-) U dr(-)S' U dr(+)ds(+) U dr(-)ds(-)
/// ```
#[derive(Debug)]
pub struct JoinPlan {
    /// The table whose rows are returned
    table_id: TableId,
    /// The table whose rows are returned
    table_name: Box<str>,
    /// The label, or alias, for the lhs table
    lhs_label: Label,
    /// The label, or alias, for the rhs table
    rhs_label: Label,
    /// The id of the lhs table
    lhs_table: TableId,
    /// The id of the rhs table
    rhs_table: TableId,
    /// An unoptimized query plan for the original view
    plan: ProjectPlan,
}

impl JoinPlan {
    /// Delta plans always return rows from a single table
    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    /// Delta plans always return rows from a single table
    pub fn table_name(&self) -> Box<str> {
        self.table_name.clone()
    }

    /// Return all tables referenced in the plan
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> {
        std::iter::once(self.lhs_table).chain(std::iter::once(self.rhs_table))
    }

    /// Returns an evaluator for computing the view delta
    pub fn evaluator<Tx: Datastore + DeltaStore>(&self, tx: &Tx) -> DeltaPlanEvaluator {
        /// Mutate a query plan by adding delta scans
        fn delta_plan(plan: &mut ProjectPlan, label: Label, delta: Delta) {
            plan.visit_mut(&mut |plan| match plan {
                PhysicalPlan::TableScan(_, var, is_delta @ None) if var == &label => {
                    *is_delta = Some(delta);
                }
                _ => {}
            });
        }

        /// Instantiate and optimize a delta plan
        fn delta_plan_opt1(plan: &ProjectPlan, label: Label, delta: Delta) -> ProjectPlan {
            let mut plan = plan.clone();
            delta_plan(&mut plan, label, delta);
            plan.optimize()
        }

        /// Instantiate and optimize a delta plan
        fn delta_plan_opt2(plan: &ProjectPlan, lhs: Label, n: Delta, rhs: Label, m: Delta) -> ProjectPlan {
            let mut plan = plan.clone();
            delta_plan(&mut plan, lhs, n);
            delta_plan(&mut plan, rhs, m);
            plan.optimize()
        }

        // dr(+)S'
        let dr_ins = tx
            .has_inserts(self.lhs_table)
            .map(|delta| delta_plan_opt1(&self.plan, self.lhs_label, delta))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // dr(-)S'
        let dr_del = tx
            .has_deletes(self.lhs_table)
            .map(|delta| delta_plan_opt1(&self.plan, self.lhs_label, delta))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // R'ds(+)
        let ds_ins = tx
            .has_inserts(self.rhs_table)
            .map(|delta| delta_plan_opt1(&self.plan, self.rhs_label, delta))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // R'ds(-)
        let ds_del = tx
            .has_deletes(self.rhs_table)
            .map(|delta| delta_plan_opt1(&self.plan, self.rhs_label, delta))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // dr(+)ds(+)
        let dr_ins_ds_ins = tx
            .has_inserts(self.lhs_table)
            .zip(tx.has_inserts(self.rhs_table))
            .map(|(n, m)| delta_plan_opt2(&self.plan, self.lhs_label, n, self.rhs_label, m))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // dr(+)ds(-)
        let dr_ins_ds_del = tx
            .has_inserts(self.lhs_table)
            .zip(tx.has_deletes(self.rhs_table))
            .map(|(n, m)| delta_plan_opt2(&self.plan, self.lhs_label, n, self.rhs_label, m))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // dr(-)ds(+)
        let dr_del_ds_ins = tx
            .has_deletes(self.lhs_table)
            .zip(tx.has_inserts(self.rhs_table))
            .map(|(n, m)| delta_plan_opt2(&self.plan, self.lhs_label, n, self.rhs_label, m))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        // dr(-)ds(-)
        let dr_del_ds_del = tx
            .has_deletes(self.lhs_table)
            .zip(tx.has_deletes(self.rhs_table))
            .map(|(n, m)| delta_plan_opt2(&self.plan, self.lhs_label, n, self.rhs_label, m))
            .map(std::iter::once)
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(std::iter::empty()));

        DeltaPlanEvaluator {
            is_join: true,
            insert_plans: ds_ins
                // R'ds(+) U dr(+)S' U dr(+)dr(-) U dr(-)ds(+)
                .chain(dr_ins)
                .chain(dr_ins_ds_del)
                .chain(dr_del_ds_ins)
                .collect(),
            delete_plans: ds_del
                // R'ds(-) U dr(-)S' U dr(+)ds(+) U dr(-)ds(-)
                .chain(dr_del)
                .chain(dr_ins_ds_ins)
                .chain(dr_del_ds_del)
                .collect(),
        }
    }
}
