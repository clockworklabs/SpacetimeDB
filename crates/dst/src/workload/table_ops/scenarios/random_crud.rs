use std::ops::Bound;

use spacetimedb_sats::AlgebraicType;

use crate::{
    schema::{generate_supported_type, ColumnPlan, SchemaPlan, TablePlan},
    seed::DstRng,
};

use super::super::{generation::ScenarioPlanner, TableWorkloadInteraction, TableWorkloadOutcome};

#[derive(Clone, Copy)]
struct ScenarioTuning {
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
}

const RANDOM_CRUD_TUNING: ScenarioTuning = ScenarioTuning {
    min_tables: 1,
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
};

const INDEXED_RANGES_TUNING: ScenarioTuning = ScenarioTuning {
    min_tables: 1,
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
};

pub fn generate_schema(rng: &mut DstRng) -> SchemaPlan {
    generate_schema_with_tuning(rng, RANDOM_CRUD_TUNING)
}

pub fn generate_indexed_ranges_schema(rng: &mut DstRng) -> SchemaPlan {
    generate_schema_with_tuning(rng, INDEXED_RANGES_TUNING)
}

fn generate_schema_with_tuning(rng: &mut DstRng, tuning: ScenarioTuning) -> SchemaPlan {
    let table_count = tuning.min_tables + rng.index(tuning.table_count_choices);
    let mut tables = Vec::with_capacity(table_count);

    for table_idx in 0..table_count {
        let extra_cols = tuning.min_extra_cols + rng.index(tuning.extra_col_choices);
        let mut columns = vec![ColumnPlan {
            name: "id".into(),
            ty: AlgebraicType::U64,
        }];
        for col_idx in 0..extra_cols {
            let ty = if col_idx < tuning.preferred_range_cols && rng.index(100) < tuning.prefer_range_compatible_pct {
                if rng.index(100) < tuning.prefer_u64_pct {
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
            && rng.index(100) < tuning.single_index_pct
        {
            extra_indexes.push(vec![col]);
        }
        if non_primary_range_cols.len() >= 2 && rng.index(100) < tuning.composite2_index_pct {
            extra_indexes.push(non_primary_range_cols[..2].to_vec());
        }
        if non_primary_range_cols.len() >= 3 && rng.index(100) < tuning.composite3_index_pct {
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

pub fn fill_pending(planner: &mut ScenarioPlanner<'_>, conn: usize) {
    fill_pending_with_tuning(planner, conn, RANDOM_CRUD_TUNING);
}

pub fn fill_pending_indexed_ranges(planner: &mut ScenarioPlanner<'_>, conn: usize) {
    fill_pending_with_tuning(planner, conn, INDEXED_RANGES_TUNING);
}

fn fill_pending_with_tuning(planner: &mut ScenarioPlanner<'_>, conn: usize, tuning: ScenarioTuning) {
    if planner.maybe_control_tx(conn, tuning.begin_tx_pct, tuning.commit_tx_pct, tuning.rollback_tx_pct) {
        return;
    }

    let table = planner.choose_table();
    let visible_rows = planner.visible_rows(conn, table);
    if emit_query(planner, conn, table, &visible_rows) {
        return;
    }
    if planner.roll_percent(5) {
        let row = planner.absent_row(conn, table);
        planner.push_interaction(TableWorkloadInteraction::delete_missing(conn, table, row));
        return;
    }
    let choose_insert = visible_rows.is_empty() || planner.roll_percent(tuning.insert_pct);
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
        planner.reinsert(conn, table, row.clone());
        planner.push_interaction(TableWorkloadInteraction::reinsert(conn, table, row));
        return;
    }

    let row = visible_rows[planner.choose_index(visible_rows.len())].clone();
    planner.delete(conn, table, row.clone());
    planner.push_interaction(TableWorkloadInteraction::delete(conn, table, row));
}

fn emit_query(
    planner: &mut ScenarioPlanner<'_>,
    conn: usize,
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
