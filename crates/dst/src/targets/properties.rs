//! Target-level property runtime shared by datastore-oriented targets.
//!
//! Properties are owned by targets (not workload generation). This keeps workloads as pure
//! operation streams and lets each target decide when and how to validate invariants.

use std::ops::Bound;

use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

use crate::schema::{SchemaPlan, SimRow};

/// Property types supported by target execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetProperty {
    InsertSelect,
    DeleteSelect,
    SelectSelectOptimizer,
    WhereTrueFalseNull,
    IndexRangeExcluded,
    BankingTablesMatch,
}

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

/// Mutable runtime state for target-owned properties.
///
/// This is intentionally small today, but it is the anchor for adding stateful
/// properties later (history windows, cross-step state, learned predicates, etc).
#[derive(Debug, Clone)]
pub(crate) struct TargetPropertyState {
    periodic_every: u64,
    enabled: Vec<TargetProperty>,
}

impl Default for TargetPropertyState {
    fn default() -> Self {
        Self {
            periodic_every: 8,
            enabled: vec![
                TargetProperty::InsertSelect,
                TargetProperty::DeleteSelect,
                TargetProperty::SelectSelectOptimizer,
                TargetProperty::WhereTrueFalseNull,
                TargetProperty::IndexRangeExcluded,
                TargetProperty::BankingTablesMatch,
            ],
        }
    }
}

impl TargetPropertyState {
    fn enabled(&self, property: TargetProperty) -> bool {
        self.enabled.contains(&property)
    }
}

pub(crate) fn on_insert<A: TargetPropertyAccess>(
    state: &TargetPropertyState,
    access: &A,
    step: u64,
    conn: usize,
    table: usize,
    row: &SimRow,
    in_tx: bool,
) -> Result<(), String> {
    if state.enabled(TargetProperty::InsertSelect) {
        check_insert_select(access, conn, table, row)?;
    }
    if !in_tx {
        maybe_run_periodic(state, access, step, table)?;
        if state.enabled(TargetProperty::BankingTablesMatch) {
            check_banking_tables_match(access)?;
        }
    }
    Ok(())
}

pub(crate) fn on_delete<A: TargetPropertyAccess>(
    state: &TargetPropertyState,
    access: &A,
    step: u64,
    conn: usize,
    table: usize,
    row: &SimRow,
    in_tx: bool,
) -> Result<(), String> {
    if state.enabled(TargetProperty::DeleteSelect) {
        check_delete_select(access, conn, table, row)?;
    }
    if !in_tx {
        maybe_run_periodic(state, access, step, table)?;
        if state.enabled(TargetProperty::BankingTablesMatch) {
            check_banking_tables_match(access)?;
        }
    }
    Ok(())
}

pub(crate) fn on_commit_or_rollback<A: TargetPropertyAccess>(
    state: &TargetPropertyState,
    access: &A,
) -> Result<(), String> {
    if state.enabled(TargetProperty::BankingTablesMatch) {
        check_banking_tables_match(access)?;
    }
    Ok(())
}

fn maybe_run_periodic<A: TargetPropertyAccess>(
    state: &TargetPropertyState,
    access: &A,
    step: u64,
    table: usize,
) -> Result<(), String> {
    if state.periodic_every == 0 || !step.is_multiple_of(state.periodic_every) {
        return Ok(());
    }
    if state.enabled(TargetProperty::SelectSelectOptimizer) {
        check_norec_select_select_optimizer(access, table)?;
    }
    if state.enabled(TargetProperty::WhereTrueFalseNull) {
        check_tlp_partitions(access, table)?;
    }
    if state.enabled(TargetProperty::IndexRangeExcluded) {
        check_index_range_excluded(access, table)?;
    }
    Ok(())
}

fn check_insert_select<A: TargetPropertyAccess>(
    access: &A,
    conn: usize,
    table: usize,
    row: &SimRow,
) -> Result<(), String> {
    let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
    let found = access.lookup_in_connection(conn, table, id)?;
    if found != Some(row.clone()) {
        return Err(format!(
            "[PQS::InsertSelect] row not visible after insert on conn={conn}, table={table}, expected={row:?}, actual={found:?}"
        ));
    }
    Ok(())
}

fn check_delete_select<A: TargetPropertyAccess>(
    access: &A,
    conn: usize,
    table: usize,
    row: &SimRow,
) -> Result<(), String> {
    let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
    if access.lookup_in_connection(conn, table, id)?.is_some() {
        return Err(format!(
            "[DeleteSelect] row still visible after delete on conn={conn}, table={table}, row={row:?}"
        ));
    }
    Ok(())
}

fn check_norec_select_select_optimizer<A: TargetPropertyAccess>(access: &A, table: usize) -> Result<(), String> {
    let table_plan = access
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
        return Ok(());
    };

    let scanned_rows = access.collect_rows_for_table(table)?;
    if scanned_rows.is_empty() {
        return Ok(());
    }

    let predicate_value = match col_ty {
        AlgebraicType::Bool => AlgebraicValue::Bool(true),
        AlgebraicType::U64 => scanned_rows[0].values[col_idx as usize].clone(),
        _ => return Ok(()),
    };
    let where_count = access.count_by_col_eq(table, col_idx, &predicate_value)?;
    let projected_true_count = scanned_rows
        .iter()
        .filter(|row| row.values[col_idx as usize] == predicate_value)
        .count();
    if where_count != projected_true_count {
        return Err(format!(
            "[NoREC::SelectSelectOptimizer] mismatch on table={table}, col={col_idx}: where_count={where_count}, projected_true={projected_true_count}"
        ));
    }
    Ok(())
}

fn check_tlp_partitions<A: TargetPropertyAccess>(access: &A, table: usize) -> Result<(), String> {
    let table_plan = access
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
        return Ok(());
    };
    let total = access.count_rows(table)?;
    let true_count = access.count_by_col_eq(table, col_idx, &AlgebraicValue::Bool(true))?;
    let false_count = access.count_by_col_eq(table, col_idx, &AlgebraicValue::Bool(false))?;
    let partition_sum = true_count + false_count;
    if partition_sum != total {
        return Err(format!(
            "[TLP::WhereTrueFalseNull|TLP::UNIONAllPreservesCardinality] partition mismatch on table={table}, col={col_idx}: true={true_count}, false={false_count}, total={total}"
        ));
    }
    Ok(())
}

fn check_index_range_excluded<A: TargetPropertyAccess>(access: &A, table: usize) -> Result<(), String> {
    let table_plan = access
        .schema_plan()
        .tables
        .get(table)
        .ok_or_else(|| format!("table {table} out of range"))?;
    let rows = access.collect_rows_for_table(table)?;
    if rows.len() < 2 {
        return Ok(());
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

        let mut actual_rows = access.range_scan(table, cols, lower, upper)?;
        actual_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));

        if actual_rows != expected_rows {
            return Err(format!(
                "[PQS::IndexRangeExcluded] range mismatch on table={table}, cols={cols:?}: expected={expected_rows:?}, actual={actual_rows:?}"
            ));
        }
    }

    Ok(())
}

fn check_banking_tables_match<A: TargetPropertyAccess>(access: &A) -> Result<(), String> {
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
