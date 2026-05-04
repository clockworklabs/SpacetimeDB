use std::ops::Bound;

use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

use crate::{
    schema::{SchemaPlan, SimRow},
    workload::table_ops::{ExpectedResult, TableScenario},
};

use super::{PropertyContext, PropertyEvent, PropertyKind, TargetPropertyAccess};

pub(super) trait PropertyRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let _ = ctx;
        let _ = event;
        Ok(())
    }
}

pub(super) fn rule_for_kind(kind: PropertyKind) -> Box<dyn PropertyRule> {
    match kind {
        PropertyKind::NotCrash => Box::<NotCrashRule>::default(),
        PropertyKind::InsertSelect => Box::<InsertSelectRule>::default(),
        PropertyKind::DeleteSelect => Box::<DeleteSelectRule>::default(),
        PropertyKind::SelectSelectOptimizer => Box::<NoRecRule>::default(),
        PropertyKind::WhereTrueFalseNull => Box::<TlpRule>::default(),
        PropertyKind::IndexRangeExcluded => Box::<IndexRangeExcludedRule>::default(),
        PropertyKind::BankingTablesMatch => Box::<BankingMatchRule>::default(),
        PropertyKind::DynamicMigrationAutoInc => Box::<DynamicMigrationAutoIncRule>::default(),
        PropertyKind::DurableReplayMatchesModel => Box::<DurableReplayMatchesModelRule>::default(),
        PropertyKind::ExpectedErrorMatches => Box::<ExpectedErrorMatchesRule>::default(),
        PropertyKind::PointLookupMatchesModel => Box::<PointLookupMatchesModelRule>::default(),
        PropertyKind::PredicateCountMatchesModel => Box::<PredicateCountMatchesModelRule>::default(),
        PropertyKind::RangeScanMatchesModel => Box::<RangeScanMatchesModelRule>::default(),
        PropertyKind::FullScanMatchesModel => Box::<FullScanMatchesModelRule>::default(),
    }
}

pub(super) fn expected_table_state_rule<S>(scenario: S, schema: SchemaPlan) -> Box<dyn PropertyRule>
where
    S: TableScenario + 'static,
{
    Box::new(ExpectedTableStateRule::new(scenario, schema))
}

#[derive(Default)]
struct NotCrashRule;

impl PropertyRule for NotCrashRule {}

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
struct DurableReplayMatchesModelRule;

impl PropertyRule for DurableReplayMatchesModelRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::DurableReplay(replay) = event else {
            return Ok(());
        };
        let expected_rows = ctx.models.table().committed_rows();
        if replay.base_rows != expected_rows {
            return Err(format!(
                "[DurableReplayMatchesModel] replayed durable state mismatch at offset {:?}: expected={expected_rows:?} actual={:?}",
                replay.durable_offset, replay.base_rows
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
