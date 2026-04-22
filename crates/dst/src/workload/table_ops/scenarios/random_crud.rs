use std::cmp::Ordering;

use spacetimedb_sats::AlgebraicType;

use crate::{
    schema::{generate_supported_type, ColumnPlan, SchemaPlan, TablePlan},
    seed::DstRng,
};

use super::super::{
    generation::ScenarioPlanner,
    properties::{property_interaction, PropertyBound, TableProperty},
    TableWorkloadOutcome,
};

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
    range_probe_pct: usize,
    in_tx_probe_pct: usize,
    composite_probe_pct: usize,
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
    range_probe_pct: 10,
    in_tx_probe_pct: 60,
    composite_probe_pct: 70,
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
    range_probe_pct: 45,
    in_tx_probe_pct: 65,
    composite_probe_pct: 90,
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
    if planner.roll_percent(tuning.range_probe_pct) && maybe_emit_range_probe(planner, conn, table, tuning) {
        return;
    }

    let visible_rows = planner.visible_rows(conn, table);
    let choose_insert = visible_rows.is_empty() || planner.roll_percent(tuning.insert_pct);
    if choose_insert {
        let row = planner.make_row(table);
        planner.insert(conn, table, row.clone());
        planner.push_interaction(super::super::TableWorkloadInteraction::Insert {
            conn,
            table,
            row: row.clone(),
        });
        planner.push_interaction(property_interaction(TableProperty::VisibleInConnection {
            conn,
            table,
            row,
        }));
        if !planner.in_tx(conn) {
            let row = planner.last_inserted_row(conn).expect("tracked auto-commit insert");
            planner.push_interaction(property_interaction(TableProperty::VisibleFresh { table, row }));
        }
        return;
    }

    let row = visible_rows[planner.choose_index(visible_rows.len())].clone();
    planner.delete(conn, table, row.clone());
    planner.push_interaction(super::super::TableWorkloadInteraction::Delete {
        conn,
        table,
        row: row.clone(),
    });
    planner.push_interaction(property_interaction(TableProperty::MissingInConnection {
        conn,
        table,
        row: row.clone(),
    }));
    if !planner.in_tx(conn) {
        planner.push_interaction(property_interaction(TableProperty::MissingFresh { table, row }));
    }
}

fn is_range_compatible(ty: &AlgebraicType) -> bool {
    matches!(ty, AlgebraicType::U64 | AlgebraicType::Bool)
}

fn maybe_emit_range_probe(
    planner: &mut ScenarioPlanner<'_>,
    conn: usize,
    table: usize,
    tuning: ScenarioTuning,
) -> bool {
    let table_plan = planner.table_plan(table);
    let mut probe_indexes = vec![vec![0]];
    probe_indexes.extend(
        table_plan
            .extra_indexes
            .iter()
            .filter(|cols| {
                cols.iter()
                    .all(|&col| is_range_compatible(&table_plan.columns[col as usize].ty))
            })
            .cloned(),
    );
    if probe_indexes.is_empty() {
        return false;
    }

    let use_connection_view = planner.in_tx(conn) && planner.roll_percent(tuning.in_tx_probe_pct);
    let basis_rows = if use_connection_view {
        planner.visible_rows(conn, table)
    } else {
        planner.committed_rows(table)
    };
    if basis_rows.is_empty() {
        return false;
    }

    let composite_indexes = probe_indexes
        .iter()
        .filter(|cols| cols.len() > 1)
        .cloned()
        .collect::<Vec<_>>();
    let cols = if !composite_indexes.is_empty() && planner.roll_percent(tuning.composite_probe_pct) {
        composite_indexes[planner.choose_index(composite_indexes.len())].clone()
    } else {
        probe_indexes[planner.choose_index(probe_indexes.len())].clone()
    };

    let lower = choose_bound(planner, &basis_rows, &cols);
    let upper = choose_bound(planner, &basis_rows, &cols);
    let (lower, upper) = normalize_bounds(lower, upper);
    let mut expected_rows = basis_rows
        .into_iter()
        .filter(|row| key_in_bounds(&row.project_key(&cols).to_algebraic_value(), &lower, &upper))
        .collect::<Vec<_>>();
    expected_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, &cols));

    let property = if use_connection_view {
        TableProperty::RangeScanInConnection {
            conn,
            table,
            cols,
            lower,
            upper,
            expected_rows,
        }
    } else {
        TableProperty::RangeScanFresh {
            table,
            cols,
            lower,
            upper,
            expected_rows,
        }
    };
    planner.push_interaction(property_interaction(property));
    true
}

fn choose_bound(planner: &mut ScenarioPlanner<'_>, rows: &[crate::schema::SimRow], cols: &[u16]) -> PropertyBound {
    if planner.roll_percent(20) {
        return PropertyBound::Unbounded;
    }
    let row = &rows[planner.choose_index(rows.len())];
    let key = row.project_key(cols);
    if planner.roll_percent(50) {
        PropertyBound::Included(key)
    } else {
        PropertyBound::Excluded(key)
    }
}

fn normalize_bounds(lower: PropertyBound, upper: PropertyBound) -> (PropertyBound, PropertyBound) {
    match (bound_key(&lower), bound_key(&upper)) {
        (Some(left), Some(right)) if left > right => (upper, lower),
        _ => (lower, upper),
    }
}

fn bound_key(bound: &PropertyBound) -> Option<spacetimedb_sats::AlgebraicValue> {
    match bound {
        PropertyBound::Unbounded => None,
        PropertyBound::Included(key) | PropertyBound::Excluded(key) => Some(key.to_algebraic_value()),
    }
}

fn key_in_bounds(key: &spacetimedb_sats::AlgebraicValue, lower: &PropertyBound, upper: &PropertyBound) -> bool {
    let lower_ok = match lower {
        PropertyBound::Unbounded => true,
        PropertyBound::Included(bound) => key >= &bound.to_algebraic_value(),
        PropertyBound::Excluded(bound) => key > &bound.to_algebraic_value(),
    };
    let upper_ok = match upper {
        PropertyBound::Unbounded => true,
        PropertyBound::Included(bound) => key <= &bound.to_algebraic_value(),
        PropertyBound::Excluded(bound) => key < &bound.to_algebraic_value(),
    };
    lower_ok && upper_ok
}

fn compare_rows_by_cols(lhs: &crate::schema::SimRow, rhs: &crate::schema::SimRow, cols: &[u16]) -> Ordering {
    lhs.project_key(cols)
        .to_algebraic_value()
        .cmp(&rhs.project_key(cols).to_algebraic_value())
        .then_with(|| lhs.values.cmp(&rhs.values))
}
