//! Target-level property runtime shared by datastore-oriented targets.
//!
//! Properties are defined once here and plugged into any target that
//! implements [`TargetPropertyAccess`].

use std::ops::Bound;

use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

use crate::schema::{SchemaPlan, SimRow};

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
}

/// Mutable runtime holding selected property implementations.
pub(crate) struct PropertyRuntime {
    rules: Vec<RuleEntry>,
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
            }
        }
        Self { rules }
    }

    pub fn on_insert(
        &mut self,
        access: &dyn TargetPropertyAccess,
        step: u64,
        conn: usize,
        table: usize,
        row: &SimRow,
        in_tx: bool,
    ) -> Result<(), String> {
        for entry in &mut self.rules {
            entry.rule.on_insert(access, step, conn, table, row, in_tx)?;
        }
        if !in_tx {
            for entry in &mut self.rules {
                if let Some(every) = entry.periodic_every()
                    && step.is_multiple_of(every)
                {
                    entry.rule.on_periodic(access, table)?;
                }
            }
        }
        Ok(())
    }

    pub fn on_delete(
        &mut self,
        access: &dyn TargetPropertyAccess,
        step: u64,
        conn: usize,
        table: usize,
        row: &SimRow,
        in_tx: bool,
    ) -> Result<(), String> {
        for entry in &mut self.rules {
            entry.rule.on_delete(access, step, conn, table, row, in_tx)?;
        }
        if !in_tx {
            for entry in &mut self.rules {
                if let Some(every) = entry.periodic_every()
                    && step.is_multiple_of(every)
                {
                    entry.rule.on_periodic(access, table)?;
                }
            }
        }
        Ok(())
    }

    pub fn on_commit_or_rollback(&mut self, access: &dyn TargetPropertyAccess) -> Result<(), String> {
        for entry in &mut self.rules {
            entry.rule.on_commit_or_rollback(access)?;
        }
        Ok(())
    }
}

struct RuleEntry {
    kind: PropertyKind,
    rule: Box<dyn PropertyRule>,
}

impl RuleEntry {
    fn new(kind: PropertyKind, rule: Box<dyn PropertyRule>) -> Self {
        Self { kind, rule }
    }

    fn periodic_every(&self) -> Option<u64> {
        match self.kind {
            PropertyKind::SelectSelectOptimizer | PropertyKind::WhereTrueFalseNull => Some(16),
            PropertyKind::IndexRangeExcluded => Some(64),
            _ => None,
        }
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
        ])
    }
}

trait PropertyRule {
    fn on_insert(
        &mut self,
        _access: &dyn TargetPropertyAccess,
        _step: u64,
        _conn: usize,
        _table: usize,
        _row: &SimRow,
        _in_tx: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    fn on_delete(
        &mut self,
        _access: &dyn TargetPropertyAccess,
        _step: u64,
        _conn: usize,
        _table: usize,
        _row: &SimRow,
        _in_tx: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    fn on_periodic(&mut self, _access: &dyn TargetPropertyAccess, _table: usize) -> Result<(), String> {
        Ok(())
    }

    fn on_commit_or_rollback(&mut self, _access: &dyn TargetPropertyAccess) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct InsertSelectRule;

impl PropertyRule for InsertSelectRule {
    fn on_insert(
        &mut self,
        access: &dyn TargetPropertyAccess,
        _step: u64,
        conn: usize,
        table: usize,
        row: &SimRow,
        _in_tx: bool,
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
}

#[derive(Default)]
struct DeleteSelectRule;

impl PropertyRule for DeleteSelectRule {
    fn on_delete(
        &mut self,
        access: &dyn TargetPropertyAccess,
        _step: u64,
        conn: usize,
        table: usize,
        row: &SimRow,
        _in_tx: bool,
    ) -> Result<(), String> {
        let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
        if access.lookup_in_connection(conn, table, id)?.is_some() {
            return Err(format!(
                "[DeleteSelect] row still visible after delete on conn={conn}, table={table}, row={row:?}"
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct NoRecRule;

impl PropertyRule for NoRecRule {
    fn on_periodic(&mut self, access: &dyn TargetPropertyAccess, table: usize) -> Result<(), String> {
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
}

#[derive(Default)]
struct TlpRule;

impl PropertyRule for TlpRule {
    fn on_periodic(&mut self, access: &dyn TargetPropertyAccess, table: usize) -> Result<(), String> {
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
}

#[derive(Default)]
struct IndexRangeExcludedRule;

impl PropertyRule for IndexRangeExcludedRule {
    fn on_periodic(&mut self, access: &dyn TargetPropertyAccess, table: usize) -> Result<(), String> {
        const MAX_ROWS_FOR_INDEX_SCAN_CHECK: usize = 512;

        let table_plan = access
            .schema_plan()
            .tables
            .get(table)
            .ok_or_else(|| format!("table {table} out of range"))?;
        let rows = access.collect_rows_for_table(table)?;
        if rows.len() < 2 || rows.len() > MAX_ROWS_FOR_INDEX_SCAN_CHECK {
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
}

#[derive(Default)]
struct BankingMatchRule;

impl PropertyRule for BankingMatchRule {
    fn on_insert(
        &mut self,
        access: &dyn TargetPropertyAccess,
        _step: u64,
        _conn: usize,
        _table: usize,
        _row: &SimRow,
        in_tx: bool,
    ) -> Result<(), String> {
        if in_tx {
            return Ok(());
        }
        check_banking_tables_match(access)
    }

    fn on_delete(
        &mut self,
        access: &dyn TargetPropertyAccess,
        _step: u64,
        _conn: usize,
        _table: usize,
        _row: &SimRow,
        in_tx: bool,
    ) -> Result<(), String> {
        if in_tx {
            return Ok(());
        }
        check_banking_tables_match(access)
    }

    fn on_commit_or_rollback(&mut self, access: &dyn TargetPropertyAccess) -> Result<(), String> {
        check_banking_tables_match(access)
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
