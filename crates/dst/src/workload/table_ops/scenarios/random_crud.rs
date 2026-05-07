use std::ops::Bound;

use spacetimedb_sats::AlgebraicType;

use crate::{
    client::SessionId,
    schema::{default_value_for_type, generate_supported_type, ColumnPlan, SchemaPlan, SimRow, TablePlan},
    seed::DstRng,
    workload::strategy::{Index, Percent, Strategy},
};

use super::super::{generation::ScenarioPlanner, TableInteractionCase, TableWorkloadInteraction, TableWorkloadOutcome};

#[derive(Clone, Copy)]
struct TableWorkloadProfile {
    min_tables: usize,
    table_count_choices: usize,
    min_extra_cols: usize,
    extra_col_choices: usize,
    preferred_range_cols: usize,
    prefer_range_compatible_pct: usize,
    prefer_u64_pct: usize,
    single_index_pct: usize,
    composite2_index_pct: usize,
    composite3_index_pct: usize,
    insert_pct: usize,
    begin_tx_pct: usize,
    commit_tx_pct: usize,
    rollback_tx_pct: usize,
    begin_read_tx_pct: usize,
    release_read_tx_pct: usize,
    empty_tx_pct: usize,
    exact_duplicate_insert_pct: usize,
    unique_key_conflict_insert_pct: usize,
    add_column_pct: usize,
    add_index_pct: usize,
}

const RANDOM_CRUD_PROFILE: TableWorkloadProfile = TableWorkloadProfile {
    min_tables: 2,
    table_count_choices: 3,
    min_extra_cols: 1,
    extra_col_choices: 4,
    preferred_range_cols: 2,
    prefer_range_compatible_pct: 65,
    prefer_u64_pct: 75,
    single_index_pct: 70,
    composite2_index_pct: 65,
    composite3_index_pct: 30,
    insert_pct: 65,
    begin_tx_pct: 20,
    commit_tx_pct: 15,
    rollback_tx_pct: 10,
    begin_read_tx_pct: 4,
    release_read_tx_pct: 35,
    empty_tx_pct: 2,
    exact_duplicate_insert_pct: 4,
    unique_key_conflict_insert_pct: 4,
    add_column_pct: 1,
    add_index_pct: 2,
};

const INDEXED_RANGES_PROFILE: TableWorkloadProfile = TableWorkloadProfile {
    min_tables: 2,
    table_count_choices: 2,
    min_extra_cols: 3,
    extra_col_choices: 3,
    preferred_range_cols: 3,
    prefer_range_compatible_pct: 90,
    prefer_u64_pct: 90,
    single_index_pct: 100,
    composite2_index_pct: 100,
    composite3_index_pct: 75,
    insert_pct: 55,
    begin_tx_pct: 20,
    commit_tx_pct: 15,
    rollback_tx_pct: 8,
    begin_read_tx_pct: 6,
    release_read_tx_pct: 30,
    empty_tx_pct: 2,
    exact_duplicate_insert_pct: 3,
    unique_key_conflict_insert_pct: 4,
    add_column_pct: 2,
    add_index_pct: 4,
};

pub fn generate_schema(rng: &mut DstRng) -> SchemaPlan {
    generate_schema_with_profile(rng, RANDOM_CRUD_PROFILE)
}

pub fn generate_indexed_ranges_schema(rng: &mut DstRng) -> SchemaPlan {
    generate_schema_with_profile(rng, INDEXED_RANGES_PROFILE)
}

fn generate_schema_with_profile(rng: &mut DstRng, profile: TableWorkloadProfile) -> SchemaPlan {
    let table_count = profile.min_tables + Index::new(profile.table_count_choices).sample(rng);
    let mut tables = Vec::with_capacity(table_count);

    for table_idx in 0..table_count {
        let extra_cols = profile.min_extra_cols + Index::new(profile.extra_col_choices).sample(rng);
        let mut columns = vec![ColumnPlan {
            name: "id".into(),
            ty: AlgebraicType::U64,
        }];
        for col_idx in 0..extra_cols {
            let ty = if col_idx < profile.preferred_range_cols
                && Percent::new(profile.prefer_range_compatible_pct).sample(rng)
            {
                if Percent::new(profile.prefer_u64_pct).sample(rng) {
                    AlgebraicType::U64
                } else {
                    AlgebraicType::Bool
                }
            } else {
                generate_supported_type(rng)
            };
            columns.push(ColumnPlan {
                name: format!("c{table_idx}_{col_idx}"),
                ty,
            });
        }
        let mut extra_indexes = Vec::new();
        let non_primary_range_cols = columns
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, col)| is_range_compatible(&col.ty))
            .map(|(idx, _)| idx as u16)
            .collect::<Vec<_>>();
        if let Some(&col) = non_primary_range_cols.first()
            && Percent::new(profile.single_index_pct).sample(rng)
        {
            extra_indexes.push(vec![col]);
        }
        if non_primary_range_cols.len() >= 2 && Percent::new(profile.composite2_index_pct).sample(rng) {
            extra_indexes.push(non_primary_range_cols[..2].to_vec());
        }
        if non_primary_range_cols.len() >= 3 && Percent::new(profile.composite3_index_pct).sample(rng) {
            extra_indexes.push(non_primary_range_cols[..3].to_vec());
        }
        extra_indexes.sort();
        extra_indexes.dedup();
        tables.push(TablePlan {
            name: format!("dst_table_{table_idx}_{}", rng.next_u64() % 10_000),
            columns,
            extra_indexes,
        });
    }

    SchemaPlan { tables }
}

pub fn validate_outcome(_schema: &SchemaPlan, _outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
    Ok(())
}

pub fn fill_pending(planner: &mut ScenarioPlanner<'_>, conn: SessionId) {
    fill_pending_with_profile(planner, conn, RANDOM_CRUD_PROFILE);
}

pub fn fill_pending_indexed_ranges(planner: &mut ScenarioPlanner<'_>, conn: SessionId) {
    fill_pending_with_profile(planner, conn, INDEXED_RANGES_PROFILE);
}

fn fill_pending_with_profile(planner: &mut ScenarioPlanner<'_>, conn: SessionId, profile: TableWorkloadProfile) {
    if planner.has_read_tx(conn) {
        let table = planner.choose_table();
        let visible_rows = planner.visible_rows(conn, table);
        if planner.roll_percent(profile.release_read_tx_pct) {
            planner.release_read_tx(conn);
            planner.push_interaction(TableWorkloadInteraction::release_read_tx(conn));
        } else if !emit_query(planner, conn, table, &visible_rows) {
            planner.push_interaction(TableWorkloadInteraction::full_scan(conn, table));
        }
        return;
    }

    if planner.active_writer().is_none() {
        if planner.roll_percent(profile.empty_tx_pct) {
            let rollback = planner.roll_percent(50);
            planner.begin_tx(conn);
            planner.push_interaction(TableWorkloadInteraction::begin_tx(conn));
            if rollback {
                planner.rollback_tx(conn);
                planner.push_interaction(TableWorkloadInteraction::rollback_tx(conn));
            } else {
                planner.commit_tx(conn);
                planner.push_interaction(TableWorkloadInteraction::commit_tx(conn));
            }
            return;
        }

        if planner.roll_percent(profile.begin_read_tx_pct) {
            planner.begin_read_tx(conn);
            planner.push_interaction(TableWorkloadInteraction::begin_read_tx(conn));
            let table = planner.choose_table();
            let visible_rows = planner.visible_rows(conn, table);
            if !emit_query(planner, conn, table, &visible_rows) {
                planner.push_interaction(TableWorkloadInteraction::full_scan(conn, table));
            }
            return;
        }
    }

    if planner.maybe_control_tx(
        conn,
        profile.begin_tx_pct,
        profile.commit_tx_pct,
        profile.rollback_tx_pct,
    ) {
        return;
    }

    let table = planner.choose_table();
    let visible_rows = planner.visible_rows(conn, table);
    if planner.active_writer().is_none()
        && !planner.any_read_tx()
        && !visible_rows.is_empty()
        && planner.roll_percent(profile.add_column_pct)
        && emit_add_column(planner, conn, table)
    {
        return;
    }
    if planner.active_writer().is_none()
        && !planner.any_read_tx()
        && visible_rows.len() >= 2
        && planner.roll_percent(profile.add_index_pct)
        && emit_add_index(planner, conn, table, &visible_rows)
    {
        return;
    }
    if emit_query(planner, conn, table, &visible_rows) {
        return;
    }
    if planner.roll_percent(5) {
        let row = planner.absent_row(conn, table);
        planner.push_interaction(TableWorkloadInteraction::delete_missing(conn, table, row));
        return;
    }
    let choose_insert = visible_rows.is_empty() || planner.roll_percent(profile.insert_pct);
    if choose_insert {
        if planner.roll_percent(10) {
            let count = 2 + planner.choose_index(3);
            let rows = (0..count).map(|_| planner.make_row(table)).collect::<Vec<_>>();
            planner.batch_insert(conn, table, &rows);
            planner.push_interaction(TableWorkloadInteraction::batch_insert(conn, table, rows));
            return;
        }
        let row = planner.make_row(table);
        planner.insert(conn, table, row.clone());
        planner.push_interaction(TableWorkloadInteraction::insert(conn, table, row));
        return;
    }

    if planner.roll_percent(profile.exact_duplicate_insert_pct) {
        let row = visible_rows[planner.choose_index(visible_rows.len())].clone();
        planner.push_interaction(TableWorkloadInteraction::exact_duplicate_insert(conn, table, row));
        return;
    }
    if planner.roll_percent(profile.unique_key_conflict_insert_pct)
        && emit_unique_key_conflict_insert(planner, conn, table, &visible_rows)
    {
        return;
    }

    if visible_rows.len() >= 2 && planner.roll_percent(10) {
        let count = 2 + planner.choose_index(visible_rows.len().min(3) - 1);
        let mut candidates = visible_rows.clone();
        let mut rows = Vec::with_capacity(count);
        for _ in 0..count {
            let idx = planner.choose_index(candidates.len());
            rows.push(candidates.remove(idx));
        }
        planner.batch_delete(conn, table, &rows);
        planner.push_interaction(TableWorkloadInteraction::batch_delete(conn, table, rows));
        return;
    }
    if planner.roll_percent(6) {
        let row = visible_rows[planner.choose_index(visible_rows.len())].clone();
        planner.delete(conn, table, row.clone());
        planner.push_interaction(TableWorkloadInteraction::delete_with_case(
            conn,
            table,
            row.clone(),
            TableInteractionCase::Reinsert,
        ));
        planner.insert(conn, table, row.clone());
        planner.push_interaction(TableWorkloadInteraction::insert(conn, table, row));
        return;
    }

    let row = visible_rows[planner.choose_index(visible_rows.len())].clone();
    planner.delete(conn, table, row.clone());
    planner.push_interaction(TableWorkloadInteraction::delete(conn, table, row));
}

fn emit_add_column(planner: &mut ScenarioPlanner<'_>, conn: SessionId, table: usize) -> bool {
    const MAX_COLUMNS_PER_TABLE: usize = 12;
    let column_idx = planner.table_plan(table).columns.len();
    if column_idx >= MAX_COLUMNS_PER_TABLE {
        return false;
    }
    let ty = match planner.choose_index(4) {
        0 => AlgebraicType::Bool,
        1 => AlgebraicType::U64,
        2 => AlgebraicType::String,
        _ => generate_supported_type_for_churn(planner),
    };
    let column = ColumnPlan {
        name: format!("dst_added_{table}_{column_idx}"),
        ty,
    };
    let default = default_value_for_type(&column.ty);
    planner.add_column(table, column.clone(), default.clone());
    planner.push_interaction(TableWorkloadInteraction::add_column(conn, table, column, default));
    true
}

fn emit_add_index(planner: &mut ScenarioPlanner<'_>, conn: SessionId, table: usize, visible_rows: &[SimRow]) -> bool {
    let candidates = candidate_new_indexes(planner, table);
    if candidates.is_empty() {
        return false;
    }
    let cols = candidates[planner.choose_index(candidates.len())].clone();
    planner.add_index(table, cols.clone());
    planner.push_interaction(TableWorkloadInteraction::add_index(conn, table, cols.clone()));
    if let Some((lower, upper)) = inclusive_bounds_for_rows(visible_rows, &cols) {
        planner.push_interaction(TableWorkloadInteraction::range_scan(
            conn,
            table,
            cols,
            Bound::Included(lower),
            Bound::Included(upper),
        ));
    }
    true
}

fn emit_unique_key_conflict_insert(
    planner: &mut ScenarioPlanner<'_>,
    conn: SessionId,
    table: usize,
    visible_rows: &[SimRow],
) -> bool {
    let source = visible_rows[planner.choose_index(visible_rows.len())].clone();
    let Some(row) = planner.unique_key_conflict_row(table, &source) else {
        return false;
    };
    planner.push_interaction(TableWorkloadInteraction::unique_key_conflict_insert(conn, table, row));
    true
}

fn generate_supported_type_for_churn(planner: &mut ScenarioPlanner<'_>) -> AlgebraicType {
    match planner.choose_index(6) {
        0 => AlgebraicType::I64,
        1 => AlgebraicType::U32,
        2 => AlgebraicType::I32,
        3 => AlgebraicType::U8,
        4 => AlgebraicType::I128,
        _ => AlgebraicType::U128,
    }
}

fn candidate_new_indexes(planner: &ScenarioPlanner<'_>, table: usize) -> Vec<Vec<u16>> {
    let table_plan = planner.table_plan(table);
    let cols = table_plan
        .columns
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, column)| is_range_compatible(&column.ty))
        .map(|(idx, _)| idx as u16)
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for width in 1..=cols.len().min(3) {
        let candidate = cols[..width].to_vec();
        if !table_plan.extra_indexes.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn inclusive_bounds_for_rows(
    rows: &[SimRow],
    cols: &[u16],
) -> Option<(spacetimedb_sats::AlgebraicValue, spacetimedb_sats::AlgebraicValue)> {
    let mut sorted = rows.to_vec();
    sorted.sort_by(|lhs, rhs| {
        lhs.project_key(cols)
            .to_algebraic_value()
            .cmp(&rhs.project_key(cols).to_algebraic_value())
            .then_with(|| lhs.values.cmp(&rhs.values))
    });
    let lower = sorted.first()?.project_key(cols).to_algebraic_value();
    let upper = sorted.last()?.project_key(cols).to_algebraic_value();
    Some((lower, upper))
}

fn emit_query(
    planner: &mut ScenarioPlanner<'_>,
    conn: SessionId,
    table: usize,
    visible_rows: &[crate::schema::SimRow],
) -> bool {
    if !planner.roll_percent(25) {
        return false;
    }
    if visible_rows.is_empty() {
        planner.push_interaction(TableWorkloadInteraction::full_scan(conn, table));
        return true;
    }

    match planner.choose_index(4) {
        0 => {
            let row = &visible_rows[planner.choose_index(visible_rows.len())];
            if let Some(id) = row.id() {
                planner.push_interaction(TableWorkloadInteraction::point_lookup(conn, table, id));
                true
            } else {
                false
            }
        }
        1 => {
            let col = choose_predicate_col(planner, table);
            let row = &visible_rows[planner.choose_index(visible_rows.len())];
            if let Some(value) = row.values.get(col as usize).cloned() {
                planner.push_interaction(TableWorkloadInteraction::predicate_count(conn, table, col, value));
                true
            } else {
                false
            }
        }
        2 => {
            let extra_indexes = planner.table_plan(table).extra_indexes.clone();
            let Some(cols) = extra_indexes
                .into_iter()
                .find(|cols| range_cols_supported(planner, table, cols))
            else {
                planner.push_interaction(TableWorkloadInteraction::full_scan(conn, table));
                return true;
            };
            let mut rows = visible_rows.to_vec();
            rows.sort_by(|lhs, rhs| {
                lhs.project_key(&cols)
                    .to_algebraic_value()
                    .cmp(&rhs.project_key(&cols).to_algebraic_value())
                    .then_with(|| lhs.values.cmp(&rhs.values))
            });
            let lower = rows[0].project_key(&cols).to_algebraic_value();
            let upper = rows[rows.len() - 1].project_key(&cols).to_algebraic_value();
            planner.push_interaction(TableWorkloadInteraction::range_scan(
                conn,
                table,
                cols,
                Bound::Included(lower),
                Bound::Included(upper),
            ));
            true
        }
        _ => {
            planner.push_interaction(TableWorkloadInteraction::full_scan(conn, table));
            true
        }
    }
}

fn choose_predicate_col(planner: &mut ScenarioPlanner<'_>, table: usize) -> u16 {
    let column_count = planner.table_plan(table).columns.len();
    if column_count <= 1 {
        0
    } else {
        1 + planner.choose_index(column_count - 1) as u16
    }
}

fn range_cols_supported(planner: &ScenarioPlanner<'_>, table: usize, cols: &[u16]) -> bool {
    cols.iter().all(|col| {
        planner
            .table_plan(table)
            .columns
            .get(*col as usize)
            .is_some_and(|column| is_range_compatible(&column.ty))
    })
}

fn is_range_compatible(ty: &AlgebraicType) -> bool {
    matches!(ty, AlgebraicType::U64 | AlgebraicType::Bool)
}
