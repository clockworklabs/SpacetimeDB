use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    client::SessionId,
    core::{StreamingProperties, TargetEngine},
    schema::{SchemaPlan, SimRow},
    workload::{
        commitlog_ops::{CommitlogInteraction, CommitlogWorkloadOutcome, DurableReplaySummary},
        table_ops::{
            PredictedOutcome, TableErrorKind, TableOracle, TableScenario, TableWorkloadInteraction,
            TableWorkloadOutcome,
        },
    },
};

use super::{
    rules::{oracle_table_state_rule, rule_for_kind, PropertyRule},
    CommitlogObservation, DynamicMigrationProbe, PropertyContext, PropertyEvent, PropertyKind, TableMutation,
    TableObservation, TargetPropertyAccess,
};

#[derive(Clone, Debug)]
pub(super) struct PropertyModels {
    table: TableModel,
}

#[derive(Clone, Debug)]
pub(super) struct TableModel {
    oracle: TableOracle,
}

impl PropertyModels {
    pub(super) fn new(table_count: usize, num_connections: usize) -> Self {
        Self {
            table: TableModel {
                oracle: TableOracle::new(table_count, num_connections),
            },
        }
    }

    pub(super) fn table(&self) -> &TableModel {
        &self.table
    }

    fn predict(&self, interaction: &TableWorkloadInteraction) -> Result<PredictedOutcome, String> {
        self.table.oracle.predict(&interaction.op)
    }

    fn apply(&mut self, interaction: &TableWorkloadInteraction) {
        self.table.oracle.apply(&interaction.op);
    }
}

impl TableModel {
    pub(super) fn committed_rows(&self) -> Vec<Vec<SimRow>> {
        self.oracle.clone().committed_rows()
    }

    pub(super) fn lookup_by_id(&self, conn: SessionId, table: usize, id: u64) -> Option<SimRow> {
        self.oracle.lookup_by_id(conn, table, id)
    }

    pub(super) fn predicate_count(&self, conn: SessionId, table: usize, col: u16, value: &AlgebraicValue) -> usize {
        self.oracle.predicate_count(conn, table, col, value)
    }

    pub(super) fn range_scan(
        &self,
        conn: SessionId,
        table: usize,
        cols: &[u16],
        lower: &Bound<AlgebraicValue>,
        upper: &Bound<AlgebraicValue>,
    ) -> Vec<SimRow> {
        self.oracle.range_scan(conn, table, cols, lower, upper)
    }

    pub(super) fn full_scan(&self, conn: SessionId, table: usize) -> Vec<SimRow> {
        let mut rows = self.oracle.visible_rows(conn, table);
        rows.sort_by_key(|row| row.id().unwrap_or_default());
        rows
    }

    pub(super) fn visible_rows(&self, conn: SessionId, table: usize) -> Vec<SimRow> {
        let mut rows = self.oracle.visible_rows(conn, table);
        rows.sort_by_key(|row| row.id().unwrap_or_default());
        rows
    }
}

/// Mutable runtime holding selected property implementations.
pub(crate) struct PropertyRuntime {
    rules: Vec<RuleEntry>,
    models: PropertyModels,
}

impl PropertyRuntime {
    pub fn with_kinds(kinds: &[PropertyKind]) -> Self {
        let rules = kinds.iter().copied().map(rule_for_kind).map(RuleEntry::new).collect();
        Self {
            rules,
            models: PropertyModels::new(0, 0),
        }
    }

    pub fn for_table_workload<S>(scenario: S, schema: SchemaPlan, num_connections: usize) -> Self
    where
        S: TableScenario + 'static,
    {
        let mut runtime = Self {
            models: PropertyModels::new(schema.tables.len(), num_connections),
            ..Self::default()
        };
        runtime
            .rules
            .push(RuleEntry::new(oracle_table_state_rule(scenario, schema)));
        runtime
    }

    pub fn on_table_interaction(
        &mut self,
        access: &dyn TargetPropertyAccess,
        interaction: &TableWorkloadInteraction,
    ) -> Result<(), String> {
        self.models.apply(interaction);
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(&ctx, PropertyEvent::TableInteractionApplied)?;
        }
        Ok(())
    }

    pub fn on_mutations(
        &mut self,
        access: &dyn TargetPropertyAccess,
        conn: SessionId,
        mutations: &[TableMutation],
        in_tx: bool,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };

        for mutation in mutations {
            match mutation {
                TableMutation::Inserted {
                    table,
                    requested: _,
                    returned,
                } => {
                    for entry in &mut self.rules {
                        entry.rule.observe(
                            &ctx,
                            PropertyEvent::RowInserted {
                                conn,
                                table: *table,
                                returned,
                                in_tx,
                            },
                        )?;
                    }
                }
                TableMutation::Deleted { table, row } => {
                    for entry in &mut self.rules {
                        entry.rule.observe(
                            &ctx,
                            PropertyEvent::RowDeleted {
                                conn,
                                table: *table,
                                row,
                                in_tx,
                            },
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn on_observed_error(
        &mut self,
        access: &dyn TargetPropertyAccess,
        observed: TableErrorKind,
        predicted: TableErrorKind,
        subject: Option<(SessionId, usize)>,
        interaction: &TableWorkloadInteraction,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::ObservedError {
                    observed,
                    predicted,
                    subject,
                    interaction,
                },
            )?;
        }
        Ok(())
    }

    pub fn on_no_mutation(
        &mut self,
        access: &dyn TargetPropertyAccess,
        subject: Option<(SessionId, usize)>,
        interaction: &TableWorkloadInteraction,
        observation: &TableObservation,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::NoMutation {
                    subject,
                    interaction,
                    observation,
                },
            )?;
        }
        Ok(())
    }

    pub fn on_point_lookup(
        &mut self,
        access: &dyn TargetPropertyAccess,
        conn: SessionId,
        table: usize,
        id: u64,
        actual: &Option<SimRow>,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::PointLookup {
                    conn,
                    table,
                    id,
                    actual,
                },
            )?;
        }
        Ok(())
    }

    pub fn on_predicate_count(
        &mut self,
        access: &dyn TargetPropertyAccess,
        conn: SessionId,
        table: usize,
        col: u16,
        value: &AlgebraicValue,
        actual: usize,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::PredicateCount {
                    conn,
                    table,
                    col,
                    value,
                    actual,
                },
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_range_scan(
        &mut self,
        access: &dyn TargetPropertyAccess,
        conn: SessionId,
        table: usize,
        cols: &[u16],
        lower: &Bound<AlgebraicValue>,
        upper: &Bound<AlgebraicValue>,
        actual: &[SimRow],
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::RangeScan {
                    conn,
                    table,
                    cols,
                    lower,
                    upper,
                    actual,
                },
            )?;
        }
        Ok(())
    }

    pub fn on_full_scan(
        &mut self,
        access: &dyn TargetPropertyAccess,
        conn: SessionId,
        table: usize,
        actual: &[SimRow],
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry
                .rule
                .observe(&ctx, PropertyEvent::FullScan { conn, table, actual })?;
        }
        Ok(())
    }

    pub fn on_commit_or_rollback(&mut self, access: &dyn TargetPropertyAccess) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(&ctx, PropertyEvent::CommitOrRollback)?;
        }
        Ok(())
    }

    pub fn on_dynamic_migration_probe(
        &mut self,
        access: &dyn TargetPropertyAccess,
        probe: &DynamicMigrationProbe,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(&ctx, PropertyEvent::DynamicMigrationProbe(probe))?;
        }
        Ok(())
    }

    pub fn on_durable_replay(
        &mut self,
        access: &dyn TargetPropertyAccess,
        replay: &DurableReplaySummary,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(&ctx, PropertyEvent::DurableReplay(replay))?;
        }
        Ok(())
    }

    pub fn on_table_workload_finish(
        &mut self,
        access: &dyn TargetPropertyAccess,
        outcome: &TableWorkloadOutcome,
    ) -> Result<(), String> {
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry
                .rule
                .observe(&ctx, PropertyEvent::TableWorkloadFinished(outcome))?;
        }
        Ok(())
    }

    fn observe_table_observation(
        &mut self,
        access: &dyn TargetPropertyAccess,
        interaction: &TableWorkloadInteraction,
        observation: &TableObservation,
    ) -> Result<(), String> {
        let prediction = self.models.predict(interaction)?;
        match (&prediction, observed_error_kind(observation)) {
            (PredictedOutcome::Error { kind, subject }, Some(observed)) => {
                self.on_observed_error(access, observed, *kind, *subject, interaction)?;
                return Ok(());
            }
            (PredictedOutcome::Error { kind, .. }, None) => {
                return Err(format!(
                    "[ErrorMatchesOracle] expected {kind:?}, observed successful result {observation:?} for {interaction:?}"
                ));
            }
            (PredictedOutcome::Applied, Some(observed)) => {
                return Err(format!(
                    "[ErrorMatchesOracle] expected success, observed {observed:?} for {interaction:?}"
                ));
            }
            (PredictedOutcome::Applied, None) => self.on_table_interaction(access, interaction)?,
            (PredictedOutcome::NoMutation { subject: _ }, Some(observed)) => {
                return Err(format!(
                    "[NoMutationMatchesModel] expected no mutation, observed {observed:?} for {interaction:?}"
                ));
            }
            (PredictedOutcome::NoMutation { subject }, None) => {
                self.on_no_mutation(access, *subject, interaction, observation)?;
            }
        }

        match observation {
            TableObservation::Applied => {}
            TableObservation::Mutated { conn, mutations, in_tx } => {
                self.on_mutations(access, *conn, mutations, *in_tx)?
            }
            TableObservation::ObservedError(_) => {}
            TableObservation::PointLookup {
                conn,
                table,
                id,
                actual,
            } => self.on_point_lookup(access, *conn, *table, *id, actual)?,
            TableObservation::PredicateCount {
                conn,
                table,
                col,
                value,
                actual,
            } => self.on_predicate_count(access, *conn, *table, *col, value, *actual)?,
            TableObservation::RangeScan {
                conn,
                table,
                cols,
                lower,
                upper,
                actual,
            } => self.on_range_scan(access, *conn, *table, cols, lower, upper, actual)?,
            TableObservation::FullScan { conn, table, actual } => self.on_full_scan(access, *conn, *table, actual)?,
            TableObservation::CommitOrRollback => {}
        }

        if matches!(observation, TableObservation::CommitOrRollback) {
            self.on_commit_or_rollback(access)?;
        }
        Ok(())
    }
}

impl<E> StreamingProperties<CommitlogInteraction, CommitlogObservation, E> for PropertyRuntime
where
    E: TargetEngine<
            CommitlogInteraction,
            Observation = CommitlogObservation,
            Outcome = CommitlogWorkloadOutcome,
            Error = String,
        > + TargetPropertyAccess,
{
    fn observe(
        &mut self,
        engine: &E,
        interaction: &CommitlogInteraction,
        observation: &CommitlogObservation,
    ) -> Result<(), String> {
        match (interaction, observation) {
            (CommitlogInteraction::Table(table_interaction), CommitlogObservation::Table(table_observation)) => {
                self.observe_table_observation(engine, table_interaction, table_observation)
            }
            (_, CommitlogObservation::DynamicMigrationProbe(probe)) => self.on_dynamic_migration_probe(engine, probe),
            (_, CommitlogObservation::DurableReplay(replay)) => self.on_durable_replay(engine, replay),
            (_, CommitlogObservation::Applied | CommitlogObservation::Skipped) => Ok(()),
            (other, observation) => Err(format!(
                "observation {observation:?} does not match interaction {other:?}"
            )),
        }
    }

    fn finish(&mut self, engine: &E, outcome: &CommitlogWorkloadOutcome) -> Result<(), String> {
        self.on_durable_replay(engine, &outcome.replay)?;
        self.on_table_workload_finish(engine, &outcome.table)
    }
}

struct RuleEntry {
    rule: Box<dyn PropertyRule>,
}

impl RuleEntry {
    fn new(rule: Box<dyn PropertyRule>) -> Self {
        Self { rule }
    }
}

impl Default for PropertyRuntime {
    fn default() -> Self {
        Self::with_kinds(&[
            PropertyKind::NotCrash,
            PropertyKind::InsertSelect,
            PropertyKind::DeleteSelect,
            PropertyKind::SelectSelectOptimizer,
            PropertyKind::WhereTrueFalseNull,
            PropertyKind::IndexRangeExcluded,
            PropertyKind::BankingTablesMatch,
            PropertyKind::DynamicMigrationAutoInc,
            PropertyKind::DurableReplayMatchesModel,
            PropertyKind::ErrorMatchesOracle,
            PropertyKind::NoMutationMatchesModel,
            PropertyKind::PointLookupMatchesModel,
            PropertyKind::PredicateCountMatchesModel,
            PropertyKind::RangeScanMatchesModel,
            PropertyKind::FullScanMatchesModel,
        ])
    }
}

fn observed_error_kind(observation: &TableObservation) -> Option<TableErrorKind> {
    match observation {
        TableObservation::ObservedError(kind) => Some(*kind),
        TableObservation::Applied
        | TableObservation::Mutated { .. }
        | TableObservation::PointLookup { .. }
        | TableObservation::PredicateCount { .. }
        | TableObservation::RangeScan { .. }
        | TableObservation::FullScan { .. }
        | TableObservation::CommitOrRollback => None,
    }
}
