use std::ops::Bound;

use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

use crate::{
    client::SessionId,
    schema::{SchemaPlan, SimRow},
    workload::table_ops::{TableOperation, TableScenario},
};

use super::{PropertyContext, PropertyEvent, PropertyKind, TableMutation, TableObservation};

pub(crate) trait PropertyRule {
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
        PropertyKind::ErrorMatchesOracle => Box::<ErrorMatchesOracleRule>::default(),
        PropertyKind::NoMutationMatchesModel => Box::<NoMutationMatchesModelRule>::default(),
        PropertyKind::PointLookupMatchesModel => Box::<PointLookupMatchesModelRule>::default(),
        PropertyKind::PredicateCountMatchesModel => Box::<PredicateCountMatchesModelRule>::default(),
        PropertyKind::RangeScanMatchesModel => Box::<RangeScanMatchesModelRule>::default(),
        PropertyKind::FullScanMatchesModel => Box::<FullScanMatchesModelRule>::default(),
    }
}

pub(crate) fn oracle_table_state_rule<S>(scenario: S, schema: SchemaPlan) -> Box<dyn PropertyRule>
where
    S: TableScenario + 'static,
{
    Box::new(OracleTableStateRule::new(scenario, schema))
}

#[derive(Default)]
struct NotCrashRule;

impl PropertyRule for NotCrashRule {}

struct OracleTableStateRule<S> {
    scenario: S,
    schema: SchemaPlan,
}

impl<S: TableScenario> OracleTableStateRule<S> {
    fn new(scenario: S, schema: SchemaPlan) -> Self {
        Self { scenario, schema }
    }
}

impl<S: TableScenario> PropertyRule for OracleTableStateRule<S> {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        match event {
            PropertyEvent::TableWorkloadFinished(outcome) => {
                let expected_rows = ctx.models.table().committed_rows();
                if outcome.final_rows != expected_rows {
                    return Err(format!(
                        "[OracleTableState] final table state mismatch: expected={expected_rows:?} actual={:?}",
                        outcome.final_rows
                    ));
                }
                self.scenario
                    .validate_outcome(&self.schema, outcome)
                    .map_err(|err| format!("[OracleTableState] scenario invariant failed: {err}"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Default)]
struct InsertSelectRule;

impl PropertyRule for InsertSelectRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::RowInserted {
            conn, table, returned, ..
        } = event
        else {
            return Ok(());
        };
        let id = returned.id().ok_or_else(|| "row missing id column".to_string())?;
        let found = ctx.access.lookup_in_connection(conn, table, id)?;
        if found != Some(returned.clone()) {
            return Err(format!(
                "[PQS::InsertSelect] row not visible after insert on conn={conn}, table={table}, expected={returned:?}, actual={found:?}"
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
struct ErrorMatchesOracleRule;

impl PropertyRule for ErrorMatchesOracleRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::ObservedError {
            observed,
            predicted,
            subject,
            interaction,
        } = event
        else {
            return Ok(());
        };
        if observed != predicted {
            return Err(format!(
                "[ErrorMatchesOracle] observed {observed:?}, but model predicted {predicted:?}: {interaction:?}",
            ));
        }
        if let Some((conn, table)) = subject {
            assert_visible_rows_match_model(ctx, conn, table, "[ErrorDoesNotMutate]", interaction)?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct NoMutationMatchesModelRule;

impl PropertyRule for NoMutationMatchesModelRule {
    fn observe(&mut self, ctx: &PropertyContext<'_>, event: PropertyEvent<'_>) -> Result<(), String> {
        let PropertyEvent::NoMutation {
            interaction,
            subject,
            observation,
        } = event
        else {
            return Ok(());
        };
        if let TableOperation::InsertRows { table, rows, .. } = &interaction.op
            && let TableObservation::Mutated { mutations, .. } = observation
        {
            if mutations.len() != rows.len() {
                return Err(format!(
                    "[NoMutationMatchesModel] insert no-op returned wrong mutation count: expected={}, actual={}; interaction={interaction:?}",
                    rows.len(),
                    mutations.len()
                ));
            }
            for (row, mutation) in rows.iter().zip(mutations) {
                let TableMutation::Inserted {
                    table: observed_table,
                    requested,
                    returned,
                } = mutation
                else {
                    return Err(format!(
                        "[NoMutationMatchesModel] insert no-op returned non-insert mutation: {mutation:?}; interaction={interaction:?}"
                    ));
                };
                if observed_table != table || requested != row || returned != row {
                    return Err(format!(
                        "[NoMutationMatchesModel] no-op insert returned row mismatch: expected table={table}, row={row:?}; observed table={observed_table}, requested={requested:?}, returned={returned:?}; interaction={interaction:?}"
                    ));
                }
            }
        }

        if let Some((conn, table)) = subject {
            assert_visible_rows_match_model(ctx, conn, table, "[NoMutationMatchesModel]", interaction)?;
        }
        Ok(())
    }
}

fn assert_visible_rows_match_model(
    ctx: &PropertyContext<'_>,
    conn: SessionId,
    table: usize,
    property: &str,
    interaction: &crate::workload::table_ops::TableWorkloadInteraction,
) -> Result<(), String> {
    let mut actual = ctx.access.collect_rows_in_connection(conn, table)?;
    actual.sort_by_key(|row| row.id().unwrap_or_default());
    let expected = ctx.models.table().visible_rows(conn, table);
    if actual != expected {
        return Err(format!(
            "{property} visible rows changed unexpectedly on conn={conn}, table={table}: expected={expected:?}, actual={actual:?}; interaction={interaction:?}"
        ));
    }
    Ok(())
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

fn compare_rows_by_cols(lhs: &SimRow, rhs: &SimRow, cols: &[u16]) -> std::cmp::Ordering {
    lhs.project_key(cols)
        .to_algebraic_value()
        .cmp(&rhs.project_key(cols).to_algebraic_value())
        .then_with(|| lhs.values.cmp(&rhs.values))
}
