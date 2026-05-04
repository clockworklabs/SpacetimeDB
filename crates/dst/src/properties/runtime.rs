use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    client::SessionId,
    core::{StreamingProperties, TargetEngine},
    schema::{SchemaPlan, SimRow},
    workload::{
        commitlog_ops::{CommitlogInteraction, CommitlogWorkloadOutcome, DurableReplaySummary},
        table_ops::{
            ExpectedErrorKind, ExpectedModel, ExpectedResult, TableOperation, TableScenario, TableWorkloadInteraction,
            TableWorkloadOutcome,
        },
    },
};

use super::{
    rules::{expected_table_state_rule, rule_for_kind, PropertyRule},
    CommitlogObservation, DynamicMigrationProbe, PropertyContext, PropertyEvent, PropertyKind, TableObservation,
    TargetPropertyAccess,
};

#[derive(Clone, Debug)]
pub(super) struct PropertyModels {
    table: TableModel,
}

#[derive(Clone, Debug)]
pub(super) struct TableModel {
    expected: ExpectedModel,
}

impl PropertyModels {
    pub(super) fn new(table_count: usize, num_connections: usize) -> Self {
        Self {
            table: TableModel {
                expected: ExpectedModel::new(table_count, num_connections),
            },
        }
    }

    pub(super) fn table(&self) -> &TableModel {
        &self.table
    }

    fn apply(&mut self, interaction: &TableWorkloadInteraction) {
        self.table.expected.apply(interaction);
    }
}

impl TableModel {
    pub(super) fn committed_rows(&self) -> Vec<Vec<SimRow>> {
        self.expected.clone().committed_rows()
    }

    pub(super) fn lookup_by_id(&self, conn: SessionId, table: usize, id: u64) -> Option<SimRow> {
        self.expected.lookup_by_id(conn, table, id)
    }

    pub(super) fn predicate_count(&self, conn: SessionId, table: usize, col: u16, value: &AlgebraicValue) -> usize {
        self.expected.predicate_count(conn, table, col, value)
    }

    pub(super) fn range_scan(
        &self,
        conn: SessionId,
        table: usize,
        cols: &[u16],
        lower: &Bound<AlgebraicValue>,
        upper: &Bound<AlgebraicValue>,
    ) -> Vec<SimRow> {
        self.expected.range_scan(conn, table, cols, lower, upper)
    }

    pub(super) fn full_scan(&self, conn: SessionId, table: usize) -> Vec<SimRow> {
        let mut rows = self.expected.visible_rows(conn, table);
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
            .push(RuleEntry::new(expected_table_state_rule(scenario, schema)));
        runtime
    }

    pub fn on_table_interaction(
        &mut self,
        access: &dyn TargetPropertyAccess,
        interaction: &TableWorkloadInteraction,
    ) -> Result<(), String> {
        match &interaction.op {
            TableOperation::BeginTx { .. }
            | TableOperation::CommitTx { .. }
            | TableOperation::RollbackTx { .. }
            | TableOperation::BeginReadTx { .. }
            | TableOperation::ReleaseReadTx { .. } => self.models.apply(interaction),
            TableOperation::BatchInsert { .. }
            | TableOperation::BatchDelete { .. }
            | TableOperation::Reinsert { .. }
            | TableOperation::AddColumn { .. }
            | TableOperation::AddIndex { .. } => self.models.apply(interaction),
            TableOperation::Insert { .. }
            | TableOperation::Delete { .. }
            | TableOperation::BeginTxConflict { .. }
            | TableOperation::WriteConflictInsert { .. }
            | TableOperation::ExactDuplicateInsert { .. }
            | TableOperation::UniqueKeyConflictInsert { .. }
            | TableOperation::DeleteMissing { .. }
            | TableOperation::PointLookup { .. }
            | TableOperation::PredicateCount { .. }
            | TableOperation::RangeScan { .. }
            | TableOperation::FullScan { .. } => {}
        }
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(&ctx, PropertyEvent::TableInteractionApplied)?;
        }
        Ok(())
    }

    pub fn on_insert(
        &mut self,
        access: &dyn TargetPropertyAccess,
        _step: u64,
        conn: SessionId,
        table: usize,
        row: &SimRow,
        in_tx: bool,
    ) -> Result<(), String> {
        self.models
            .apply(&TableWorkloadInteraction::insert(conn, table, row.clone()));
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::RowInserted {
                    conn,
                    table,
                    row,
                    in_tx,
                },
            )?;
        }
        Ok(())
    }

    pub fn on_delete(
        &mut self,
        access: &dyn TargetPropertyAccess,
        _step: u64,
        conn: SessionId,
        table: usize,
        row: &SimRow,
        in_tx: bool,
    ) -> Result<(), String> {
        self.models
            .apply(&TableWorkloadInteraction::delete(conn, table, row.clone()));
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry.rule.observe(
                &ctx,
                PropertyEvent::RowDeleted {
                    conn,
                    table,
                    row,
                    in_tx,
                },
            )?;
        }
        Ok(())
    }

    pub fn on_expected_error(
        &mut self,
        access: &dyn TargetPropertyAccess,
        kind: ExpectedErrorKind,
        interaction: &TableWorkloadInteraction,
    ) -> Result<(), String> {
        if interaction.expected != ExpectedResult::Err(kind) {
            return Err(format!(
                "[ExpectedErrorMatches] expected {:?}, observed {kind:?} for {interaction:?}",
                interaction.expected
            ));
        }
        let ctx = PropertyContext {
            access,
            models: &self.models,
        };
        for entry in &mut self.rules {
            entry
                .rule
                .observe(&ctx, PropertyEvent::ExpectedError { kind, interaction })?;
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
        match observation {
            TableObservation::Applied => {}
            TableObservation::RowInserted {
                conn,
                table,
                row,
                in_tx,
            } => self.on_insert(access, 0, *conn, *table, row, *in_tx)?,
            TableObservation::RowDeleted {
                conn,
                table,
                row,
                in_tx,
            } => self.on_delete(access, 0, *conn, *table, row, *in_tx)?,
            TableObservation::ExpectedError(kind) => self.on_expected_error(access, *kind, interaction)?,
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

        self.on_table_interaction(access, interaction)?;

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
            PropertyKind::ExpectedErrorMatches,
            PropertyKind::PointLookupMatchesModel,
            PropertyKind::PredicateCountMatchesModel,
            PropertyKind::RangeScanMatchesModel,
            PropertyKind::FullScanMatchesModel,
        ])
    }
}
