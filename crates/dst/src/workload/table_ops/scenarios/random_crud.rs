use spacetimedb_sats::AlgebraicType;

use crate::{
    schema::{generate_supported_type, ColumnPlan, SchemaPlan, TablePlan},
    seed::DstRng,
};

use super::super::{
    generation::ScenarioPlanner,
    properties::{property_interaction, TableProperty},
    TableWorkloadOutcome,
};

pub fn generate_schema(rng: &mut DstRng) -> SchemaPlan {
    let table_count = rng.index(3) + 1;
    let mut tables = Vec::with_capacity(table_count);

    for table_idx in 0..table_count {
        let extra_cols = rng.index(3);
        let mut columns = vec![ColumnPlan {
            name: "id".into(),
            ty: AlgebraicType::U64,
        }];
        for col_idx in 0..extra_cols {
            columns.push(ColumnPlan {
                name: format!("c{table_idx}_{col_idx}"),
                ty: generate_supported_type(rng),
            });
        }
        let secondary_index_col = (columns.len() > 1 && rng.index(100) < 50).then_some(1);
        tables.push(TablePlan {
            name: format!("dst_table_{table_idx}_{}", rng.next_u64() % 10_000),
            columns,
            secondary_index_col,
        });
    }

    SchemaPlan { tables }
}

pub fn validate_outcome(_schema: &SchemaPlan, _outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
    Ok(())
}

pub fn fill_pending(planner: &mut ScenarioPlanner<'_>, conn: usize) {
    if planner.maybe_control_tx(conn, 20, 15, 10) {
        return;
    }

    let table = planner.choose_table();
    let visible_rows = planner.visible_rows(conn, table);
    let choose_insert = visible_rows.is_empty() || planner.roll_percent(65);
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
