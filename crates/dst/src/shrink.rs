use crate::datastore_sim::{failure_reason, DatastoreExecutionFailure, DatastoreSimulatorCase, Interaction};

pub fn shrink_failure(
    case: &DatastoreSimulatorCase,
    failure: &DatastoreExecutionFailure,
) -> anyhow::Result<DatastoreSimulatorCase> {
    let mut shrunk = case.clone();
    shrunk.interactions.truncate(failure.step_index.saturating_add(1));
    let target_reason = failure.reason.clone();

    let mut changed = true;
    while changed {
        changed = false;
        for idx in (0..shrunk.interactions.len()).rev() {
            let Some(candidate) = remove_interaction(&shrunk, idx) else {
                continue;
            };
            if failure_reason(&candidate).ok().as_ref() == Some(&target_reason) {
                shrunk = candidate;
                changed = true;
            }
        }
    }

    Ok(shrunk)
}

fn remove_interaction(case: &DatastoreSimulatorCase, idx: usize) -> Option<DatastoreSimulatorCase> {
    let interaction = case.interactions.get(idx)?;
    if matches!(
        interaction,
        Interaction::CommitTx { .. } | Interaction::RollbackTx { .. }
    ) {
        return None;
    }

    let mut interactions = case.interactions.clone();
    interactions.remove(idx);
    Some(DatastoreSimulatorCase {
        seed: case.seed,
        num_connections: case.num_connections,
        schema: case.schema.clone(),
        interactions,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        datastore_sim::{
            run_case_detailed, ColumnKind, ColumnPlan, DatastoreSimulatorCase, Interaction, SchemaPlan, SimRow,
            SimValue, TablePlan,
        },
        seed::DstSeed,
        shrink::shrink_failure,
    };

    #[test]
    fn shrink_drops_trailing_noise() {
        let case = DatastoreSimulatorCase {
            seed: DstSeed(77),
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            kind: ColumnKind::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            kind: ColumnKind::String,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![
                Interaction::Insert {
                    conn: 0,
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::AssertVisibleFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::AssertMissingFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::Insert {
                    conn: 0,
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(2), SimValue::String("two".into())],
                    },
                },
            ],
        };

        let failure = run_case_detailed(&case).expect_err("case should fail");
        let shrunk = shrink_failure(&case, &failure).expect("shrink failure");
        assert!(shrunk.interactions.len() < case.interactions.len());
        let shrunk_failure = run_case_detailed(&shrunk).expect_err("shrunk case should still fail");
        assert_eq!(shrunk_failure.reason, failure.reason);
    }
}
