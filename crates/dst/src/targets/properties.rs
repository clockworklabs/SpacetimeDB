//! Target-level property runtime shared by table-oriented targets.
//!
//! Properties are defined once here and plugged into any target that
//! implements [`TargetPropertyAccess`].

use std::ops::Bound;

use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

use crate::{
    schema::{SchemaPlan, SimRow},
    workload::table_ops::{
        ExpectedErrorKind, ExpectedModel, ExpectedResult, TableOperation, TableScenario, TableWorkloadInteraction,
        TableWorkloadOutcome,
    },
};

/// Target adapter for property evaluation.
pub(crate) trait TargetPropertyAccess {
    fn schema_plan(&self) -> &SchemaPlan;
    fn lookup_in_connection(&self, conn: usize, table: usize, id: u64) -> Result<Option<SimRow>, String>;
    fn collect_rows_for_table(&self, table: usize) -> Result<Vec<SimRow>, String>;
    fn count_rows(&self, table: usize) -> Result<usize, String>;
    fn count_by_col_eq(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String>;
    fn range_scan(
        &self,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String>;
}

/// Canonical property IDs that can be selected by targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyKind {
    InsertSelect,
    DeleteSelect,
    SelectSelectOptimizer,
    WhereTrueFalseNull,
    IndexRangeExcluded,
    BankingTablesMatch,
    DynamicMigrationAutoInc,
    ExpectedErrorMatches,
    PointLookupMatchesModel,
    PredicateCountMatchesModel,
    RangeScanMatchesModel,
    FullScanMatchesModel,
}

#[derive(Clone, Debug)]
pub(crate) struct DynamicMigrationProbe {
    pub slot: u32,
    pub from_version: u32,
    pub to_version: u32,
    pub existing_rows: Vec<SimRow>,
    pub inserted_row: SimRow,
}

#[derive(Clone, Debug)]
pub(crate) struct PropertyModels {
    table: TableModel,
}

#[derive(Clone, Debug)]
pub(crate) struct TableModel {
    expected: ExpectedModel,
}

pub(crate) struct PropertyContext<'a> {
    pub access: &'a dyn TargetPropertyAccess,
    pub models: &'a PropertyModels,
}

#[derive(Clone, Debug)]
pub(crate) enum PropertyEvent<'a> {
    TableInteractionApplied,
    RowInserted {
        conn: usize,
        table: usize,
        row: &'a SimRow,
        in_tx: bool,
    },
    RowDeleted {
        conn: usize,
        table: usize,
        row: &'a SimRow,
        in_tx: bool,
    },
    ExpectedError {
        kind: ExpectedErrorKind,
        interaction: &'a TableWorkloadInteraction,
    },
    PointLookup {
        conn: usize,
        table: usize,
        id: u64,
        actual: &'a Option<SimRow>,
    },
    PredicateCount {
        conn: usize,
        table: usize,
        col: u16,
        value: &'a AlgebraicValue,
        actual: usize,
    },
    RangeScan {
        conn: usize,
        table: usize,
        cols: &'a [u16],
        lower: &'a Bound<AlgebraicValue>,
        upper: &'a Bound<AlgebraicValue>,
        actual: &'a [SimRow],
    },
    FullScan {
        conn: usize,
        table: usize,
        actual: &'a [SimRow],
    },
    CommitOrRollback,
    DynamicMigrationProbe(&'a DynamicMigrationProbe),
    TableWorkloadFinished(&'a TableWorkloadOutcome),
}

impl PropertyModels {
    pub fn new(table_count: usize, num_connections: usize) -> Self {
        Self {
            table: TableModel {
                expected: ExpectedModel::new(table_count, num_connections),
            },
        }
    }

    pub fn table(&self) -> &TableModel {
        &self.table
    }

    fn apply(&mut self, interaction: &TableWorkloadInteraction) {
        self.table.expected.apply(interaction);
    }
}

impl TableModel {
    pub fn committed_rows(&self) -> Vec<Vec<SimRow>> {
        self.expected.clone().committed_rows()
    }

    pub fn lookup_by_id(&self, conn: usize, table: usize, id: u64) -> Option<SimRow> {
        self.expected.lookup_by_id(conn, table, id)
    }

    pub fn predicate_count(&self, conn: usize, table: usize, col: u16, value: &AlgebraicValue) -> usize {
        self.expected.predicate_count(conn, table, col, value)
    }

    pub fn range_scan(
        &self,
        conn: usize,
        table: usize,
        cols: &[u16],
        lower: &Bound<AlgebraicValue>,
        upper: &Bound<AlgebraicValue>,
    ) -> Vec<SimRow> {
        self.expected.range_scan(conn, table, cols, lower, upper)
    }

    pub fn full_scan(&self, conn: usize, table: usize) -> Vec<SimRow> {
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
        let mut rules: Vec<RuleEntry> = Vec::with_capacity(kinds.len());
        for kind in kinds {
            match kind {
                PropertyKind::InsertSelect => rules.push(RuleEntry::new(*kind, Box::<InsertSelectRule>::default())),
                PropertyKind::DeleteSelect => rules.push(RuleEntry::new(*kind, Box::<DeleteSelectRule>::default())),
                PropertyKind::SelectSelectOptimizer => rules.push(RuleEntry::new(*kind, Box::<NoRecRule>::default())),
                PropertyKind::WhereTrueFalseNull => rules.push(RuleEntry::new(*kind, Box::<TlpRule>::default())),
                PropertyKind::IndexRangeExcluded => {
                    rules.push(RuleEntry::new(*kind, Box::<IndexRangeExcludedRule>::default()))
                }
                PropertyKind::BankingTablesMatch => {
                    rules.push(RuleEntry::new(*kind, Box::<BankingMatchRule>::default()))
                }
                PropertyKind::DynamicMigrationAutoInc => {
                    rules.push(RuleEntry::new(*kind, Box::<DynamicMigrationAutoIncRule>::default()))
                }
                PropertyKind::ExpectedErrorMatches => {
                    rules.push(RuleEntry::new(*kind, Box::<ExpectedErrorMatchesRule>::default()))
                }
                PropertyKind::PointLookupMatchesModel => {
                    rules.push(RuleEntry::new(*kind, Box::<PointLookupMatchesModelRule>::default()))
                }
                PropertyKind::PredicateCountMatchesModel => {
                    rules.push(RuleEntry::new(*kind, Box::<PredicateCountMatchesModelRule>::default()))
                }
                PropertyKind::RangeScanMatchesModel => {
                    rules.push(RuleEntry::new(*kind, Box::<RangeScanMatchesModelRule>::default()))
                }
                PropertyKind::FullScanMatchesModel => {
                    rules.push(RuleEntry::new(*kind, Box::<FullScanMatchesModelRule>::default()))
                }
            }
        }
        Self {
            rules,
            models: PropertyModels::new(0, 0),
        }
    }

    pub fn for_table_workload<S>(scenario: S, schema: SchemaPlan, num_connections: usize) -> Self
    where
        S: TableScenario + 'static,
    {
        let mut runtime = Self::default();
        runtime.models = PropertyModels::new(schema.tables.len(), num_connections);
        runtime
            .rules
            .push(RuleEntry::non_periodic(Box::new(ExpectedTableStateRule::new(
                scenario, schema,
            ))));
        runtime
    }

    pub fn on_table_interaction(
        &mut self,
        access: &dyn TargetPropertyAccess,
        interaction: &TableWorkloadInteraction,
    ) -> Result<(), String> {
        match &interaction.op {
            TableOperation::BeginTx { .. } | TableOperation::CommitTx { .. } | TableOperation::RollbackTx { .. } => {
                self.models.apply(interaction)
            }
            TableOperation::BatchInsert { .. }
            | TableOperation::BatchDelete { .. }
            | TableOperation::Reinsert { .. } => self.models.apply(interaction),
            TableOperation::Insert { .. }
            | TableOperation::Delete { .. }
            | TableOperation::DuplicateInsert { .. }
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
        conn: usize,
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
        conn: usize,
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
        conn: usize,
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
        conn: usize,
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

    pub fn on_range_scan(
        &mut self,
        access: &dyn TargetPropertyAccess,
        conn: usize,
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
        conn: usize,
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
}

struct RuleEntry {
    rule: Box<dyn PropertyRule>,
}

impl RuleEntry {
    fn new(kind: PropertyKind, rule: Box<dyn PropertyRule>) -> Self {
        let _ = kind;
        Self { rule }
    }

    fn non_periodic(rule: Box<dyn PropertyRule>) -> Self {
        Self { rule }
    }
}

impl Default for PropertyRuntime {
    fn default() -> Self {
        Self::with_kinds(&[
            PropertyKind::InsertSelect,
            PropertyKind::DeleteSelect,
            PropertyKind::SelectSelectOptimizer,
            PropertyKind::WhereTrueFalseNull,
            PropertyKind::IndexRangeExcluded,
            PropertyKind::BankingTablesMatch,
            PropertyKind::DynamicMigrationAutoInc,
            PropertyKind::ExpectedErrorMatches,
            PropertyKind::PointLookupMatchesModel,
            PropertyKind::PredicateCountMatchesModel,
            PropertyKind::RangeScanMatchesModel,
            PropertyKind::FullScanMatchesModel,
        ])
    }
}

trait PropertyRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let _ = ctx;
        let _ = event;
        Ok(())
    }
}

struct ExpectedTableStateRule<S> {
    scenario: S,
    schema: SchemaPlan,
}

impl<S: TableScenario> ExpectedTableStateRule<S> {
    fn new(scenario: S, schema: SchemaPlan) -> Self {
        Self { scenario, schema }
    }
}

impl<S: TableScenario> PropertyRule for ExpectedTableStateRule<S> {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        match event {
            PropertyEvent::TableWorkloadFinished(outcome) => {
                let expected_rows = ctx.models.table().committed_rows();
                if outcome.final_rows != expected_rows {
                    return Err(format!(
                        "[ExpectedTableState] final table state mismatch: expected={expected_rows:?} actual={:?}",
                        outcome.final_rows
                    ));
                }
                self.scenario
                    .validate_outcome(&self.schema, outcome)
                    .map_err(|err| format!("[ExpectedTableState] scenario invariant failed: {err}"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Default)]
struct InsertSelectRule;

impl PropertyRule for InsertSelectRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::RowInserted { conn, table, row, .. } = event else {
            return Ok(());
        };
        let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
        let found = ctx.access.lookup_in_connection(conn, table, id)?;
        if found != Some(row.clone()) {
            return Err(format!(
                "[PQS::InsertSelect] row not visible after insert on conn={conn}, table={table}, expected={row:?}, actual={found:?}"
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct DeleteSelectRule;

impl PropertyRule for DeleteSelectRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::RowDeleted { conn, table, row, .. } = event else {
            return Ok(());
        };
        let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
        if ctx.access.lookup_in_connection(conn, table, id)?.is_some() {
            return Err(format!(
                "[DeleteSelect] row still visible after delete on conn={conn}, table={table}, row={row:?}"
            ));
        }
        Ok(())
    }
}

fn post_write_check_tables(ctx: &PropertyContext<'_>, event: &PropertyEvent<'_>) -> Option<Vec<usize>> {
    match event {
        PropertyEvent::RowInserted {
            table, in_tx: false, ..
        }
        | PropertyEvent::RowDeleted {
            table, in_tx: false, ..
        } => Some(vec![*table]),
        PropertyEvent::CommitOrRollback => Some((0..ctx.access.schema_plan().tables.len()).collect()),
        _ => None,
    }
}

#[derive(Default)]
struct NoRecRule;

impl PropertyRule for NoRecRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let Some(tables) = post_write_check_tables(ctx, &event) else {
            return Ok(());
        };
        for table in tables {
            let table_plan = ctx
                .access
                .schema_plan()
                .tables
                .get(table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let Some((col_idx, col_ty)) = table_plan
                .columns
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, col)| matches!(col.ty, AlgebraicType::Bool | AlgebraicType::U64))
                .map(|(idx, col)| (idx as u16, &col.ty))
            else {
                continue;
            };
            let scanned_rows = ctx.access.collect_rows_for_table(table)?;
            if scanned_rows.is_empty() {
                continue;
            }
            let predicate_value = match col_ty {
                AlgebraicType::Bool => AlgebraicValue::Bool(true),
                AlgebraicType::U64 => scanned_rows[0].values[col_idx as usize].clone(),
                _ => continue,
            };
            let where_count = ctx.access.count_by_col_eq(table, col_idx, &predicate_value)?;
            let projected_true_count = scanned_rows
                .iter()
                .filter(|row| row.values[col_idx as usize] == predicate_value)
                .count();
            if where_count != projected_true_count {
                return Err(format!(
                    "[NoREC::SelectSelectOptimizer] mismatch on table={table}, col={col_idx}: where_count={where_count}, projected_true={projected_true_count}"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct TlpRule;

impl PropertyRule for TlpRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let Some(tables) = post_write_check_tables(ctx, &event) else {
            return Ok(());
        };
        for table in tables {
            let table_plan = ctx
                .access
                .schema_plan()
                .tables
                .get(table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let Some(col_idx) = table_plan
                .columns
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, col)| matches!(col.ty, AlgebraicType::Bool))
                .map(|(idx, _)| idx as u16)
            else {
                continue;
            };
            let total = ctx.access.count_rows(table)?;
            let true_count = ctx
                .access
                .count_by_col_eq(table, col_idx, &AlgebraicValue::Bool(true))?;
            let false_count = ctx
                .access
                .count_by_col_eq(table, col_idx, &AlgebraicValue::Bool(false))?;
            let partition_sum = true_count + false_count;
            if partition_sum != total {
                return Err(format!(
                    "[TLP::WhereTrueFalseNull|TLP::UNIONAllPreservesCardinality] partition mismatch on table={table}, col={col_idx}: true={true_count}, false={false_count}, total={total}"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct IndexRangeExcludedRule;

impl PropertyRule for IndexRangeExcludedRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let Some(tables) = post_write_check_tables(ctx, &event) else {
            return Ok(());
        };
        const MAX_ROWS_FOR_INDEX_SCAN_CHECK: usize = 512;

        for table in tables {
            let table_plan = ctx
                .access
                .schema_plan()
                .tables
                .get(table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let rows = ctx.access.collect_rows_for_table(table)?;
            if rows.len() < 2 || rows.len() > MAX_ROWS_FOR_INDEX_SCAN_CHECK {
                continue;
            }

            for cols in table_plan.extra_indexes.iter().filter(|cols| cols.len() > 1) {
                if !cols.iter().all(|&col| {
                    matches!(
                        table_plan.columns[col as usize].ty,
                        AlgebraicType::U64 | AlgebraicType::Bool
                    )
                }) {
                    continue;
                }

                let mut sorted_rows = rows.clone();
                sorted_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));

                let lower_key = sorted_rows[0].project_key(cols).to_algebraic_value();
                let upper_key = sorted_rows[sorted_rows.len() - 1]
                    .project_key(cols)
                    .to_algebraic_value();
                let lower = Bound::Included(lower_key.clone());
                let upper = Bound::Excluded(upper_key.clone());

                let mut expected_rows = sorted_rows
                    .into_iter()
                    .filter(|row| {
                        let key = row.project_key(cols).to_algebraic_value();
                        key >= lower_key && key < upper_key
                    })
                    .collect::<Vec<_>>();
                expected_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));

                let mut actual_rows = ctx.access.range_scan(table, cols, lower, upper)?;
                actual_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));

                if actual_rows != expected_rows {
                    return Err(format!(
                        "[PQS::IndexRangeExcluded] range mismatch on table={table}, cols={cols:?}: expected={expected_rows:?}, actual={actual_rows:?}"
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Default)]
struct BankingMatchRule;

impl PropertyRule for BankingMatchRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        match event {
            PropertyEvent::RowInserted { in_tx: false, .. }
            | PropertyEvent::RowDeleted { in_tx: false, .. }
            | PropertyEvent::CommitOrRollback => check_banking_tables_match(ctx.access),
            _ => Ok(()),
        }
    }
}

#[derive(Default)]
struct DynamicMigrationAutoIncRule;

impl PropertyRule for DynamicMigrationAutoIncRule {
    fn observe(&mut self, _ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::DynamicMigrationProbe(probe) = event else {
            return Ok(());
        };
        let max_existing_id = probe
            .existing_rows
            .iter()
            .filter_map(sim_row_integer_id)
            .max()
            .unwrap_or(0);
        let inserted_id = sim_row_integer_id(&probe.inserted_row).ok_or_else(|| {
            format!(
                "[DynamicMigrationAutoInc] probe row missing integer id for slot={}, from_version={}, to_version={}: {:?}",
                probe.slot, probe.from_version, probe.to_version, probe.inserted_row
            )
        })?;
        if inserted_id <= max_existing_id {
            return Err(format!(
                "[DynamicMigrationAutoInc] non-advancing id for slot={}, from_version={}, to_version={}: inserted_id={}, max_existing_id={}",
                probe.slot, probe.from_version, probe.to_version, inserted_id, max_existing_id
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct ExpectedErrorMatchesRule;

impl PropertyRule for ExpectedErrorMatchesRule {
    fn observe(&mut self, _ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::ExpectedError { kind, interaction } = event else {
            return Ok(());
        };
        if interaction.expected == ExpectedResult::Err(kind) {
            Ok(())
        } else {
            Err(format!(
                "[ExpectedErrorMatches] observed {kind:?}, but interaction expected {:?}: {interaction:?}",
                interaction.expected
            ))
        }
    }
}

#[derive(Default)]
struct PointLookupMatchesModelRule;

impl PropertyRule for PointLookupMatchesModelRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::PointLookup {
            conn,
            table,
            id,
            actual,
        } = event
        else {
            return Ok(());
        };
        let expected = ctx.models.table().lookup_by_id(conn, table, id);
        if *actual != expected {
            return Err(format!(
                "[Model::PointLookup] mismatch conn={conn}, table={table}, id={id}: expected={expected:?}, actual={actual:?}"
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct PredicateCountMatchesModelRule;

impl PropertyRule for PredicateCountMatchesModelRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::PredicateCount {
            conn,
            table,
            col,
            value,
            actual,
        } = event
        else {
            return Ok(());
        };
        let expected = ctx.models.table().predicate_count(conn, table, col, value);
        if actual != expected {
            return Err(format!(
                "[Model::PredicateCount] mismatch conn={conn}, table={table}, col={col}, value={value:?}: expected={expected}, actual={actual}"
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct RangeScanMatchesModelRule;

impl PropertyRule for RangeScanMatchesModelRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::RangeScan {
            conn,
            table,
            cols,
            lower,
            upper,
            actual,
        } = event
        else {
            return Ok(());
        };
        let expected = ctx.models.table().range_scan(conn, table, cols, lower, upper);
        if actual != expected.as_slice() {
            return Err(format!(
                "[Model::RangeScan] mismatch conn={conn}, table={table}, cols={cols:?}, lower={lower:?}, upper={upper:?}: expected={expected:?}, actual={actual:?}"
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct FullScanMatchesModelRule;

impl PropertyRule for FullScanMatchesModelRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::FullScan { conn, table, actual } = event else {
            return Ok(());
        };
        let expected = ctx.models.table().full_scan(conn, table);
        if actual != expected.as_slice() {
            return Err(format!(
                "[Model::FullScan] mismatch conn={conn}, table={table}: expected={expected:?}, actual={actual:?}"
            ));
        }
        Ok(())
    }
}

fn check_banking_tables_match(access: &dyn TargetPropertyAccess) -> Result<(), String> {
    let schema = access.schema_plan();
    let debit = schema.tables.iter().position(|table| table.name == "debit_accounts");
    let credit = schema.tables.iter().position(|table| table.name == "credit_accounts");
    let (Some(left), Some(right)) = (debit, credit) else {
        return Ok(());
    };

    let left_rows = access.collect_rows_for_table(left)?;
    let right_rows = access.collect_rows_for_table(right)?;
    if left_rows != right_rows {
        return Err(format!(
            "[Shadow::AllTableHaveExpectedContent] banking mismatch: debit={left_rows:?}, credit={right_rows:?}"
        ));
    }
    Ok(())
}

fn compare_rows_by_cols(lhs: &SimRow, rhs: &SimRow, cols: &[u16]) -> std::cmp::Ordering {
    lhs.project_key(cols)
        .to_algebraic_value()
        .cmp(&rhs.project_key(cols).to_algebraic_value())
        .then_with(|| lhs.values.cmp(&rhs.values))
}

fn sim_row_integer_id(row: &SimRow) -> Option<i128> {
    match row.values.first() {
        Some(AlgebraicValue::I64(value)) => Some(*value as i128),
        Some(AlgebraicValue::U64(value)) => Some(*value as i128),
        _ => None,
    }
}
