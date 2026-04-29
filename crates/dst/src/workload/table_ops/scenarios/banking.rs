use spacetimedb_sats::AlgebraicType;

use crate::schema::{ColumnPlan, SchemaPlan, TablePlan};

use super::super::{generation::ScenarioPlanner, TableWorkloadInteraction, TableWorkloadOutcome};

pub fn generate_schema() -> SchemaPlan {
    SchemaPlan {
        tables: vec![
            TablePlan {
                name: "debit_accounts".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: AlgebraicType::U64,
                    },
                    ColumnPlan {
                        name: "balance".into(),
                        ty: AlgebraicType::U64,
                    },
                ],
                extra_indexes: vec![vec![1]],
            },
            TablePlan {
                name: "credit_accounts".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: AlgebraicType::U64,
                    },
                    ColumnPlan {
                        name: "balance".into(),
                        ty: AlgebraicType::U64,
                    },
                ],
                extra_indexes: vec![vec![1]],
            },
        ],
    }
}

pub fn validate_outcome(schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
    let debit_idx = schema
        .tables
        .iter()
        .position(|table| table.name == "debit_accounts")
        .ok_or_else(|| anyhow::anyhow!("missing debit_accounts table"))?;
    let credit_idx = schema
        .tables
        .iter()
        .position(|table| table.name == "credit_accounts")
        .ok_or_else(|| anyhow::anyhow!("missing credit_accounts table"))?;

    let debit_rows = outcome
        .final_rows
        .get(debit_idx)
        .ok_or_else(|| anyhow::anyhow!("missing debit_accounts rows"))?;
    let credit_rows = outcome
        .final_rows
        .get(credit_idx)
        .ok_or_else(|| anyhow::anyhow!("missing credit_accounts rows"))?;

    if debit_rows != credit_rows {
        anyhow::bail!("banking tables diverged: debit={debit_rows:?} credit={credit_rows:?}");
    }
    Ok(())
}

pub fn fill_pending(planner: &mut ScenarioPlanner<'_>, conn: usize) {
    if planner.maybe_control_tx(conn, 25, 20, 10) {
        return;
    }

    let debit_rows = planner.visible_rows(conn, 0);
    let choose_insert = debit_rows.is_empty() || planner.roll_percent(65);
    if choose_insert {
        let row = planner.make_row(0);
        let mirror = row.clone();
        planner.insert(conn, 0, row.clone());
        planner.insert(conn, 1, mirror.clone());
        planner.push_interaction(TableWorkloadInteraction::insert(conn, 0, row.clone()));
        planner.push_interaction(TableWorkloadInteraction::insert(conn, 1, mirror.clone()));
        return;
    }

    let row = debit_rows[planner.choose_index(debit_rows.len())].clone();
    let mirror = row.clone();
    planner.delete(conn, 0, row.clone());
    planner.delete(conn, 1, mirror.clone());
    planner.push_interaction(TableWorkloadInteraction::delete(conn, 0, row.clone()));
    planner.push_interaction(TableWorkloadInteraction::delete(conn, 1, mirror.clone()));
}
