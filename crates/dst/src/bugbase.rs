use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::datastore_sim::{DatastoreExecutionFailure, DatastoreSimulatorCase};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreBugArtifact {
    pub seed: u64,
    pub failure: DatastoreExecutionFailure,
    pub case: DatastoreSimulatorCase,
    pub shrunk_case: Option<DatastoreSimulatorCase>,
}

pub fn save_bug_artifact(path: impl AsRef<Path>, artifact: &DatastoreBugArtifact) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(artifact)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn load_bug_artifact(path: impl AsRef<Path>) -> anyhow::Result<DatastoreBugArtifact> {
    let body = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{
        bugbase::{load_bug_artifact, save_bug_artifact, DatastoreBugArtifact},
        datastore_sim::{
            run_case_detailed, ColumnKind, ColumnPlan, DatastoreSimulatorCase, Interaction, SchemaPlan, SimRow,
            SimValue, TablePlan,
        },
        seed::DstSeed,
    };

    #[test]
    fn bug_artifact_roundtrips() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("bug.json");
        let case = DatastoreSimulatorCase {
            seed: DstSeed(5),
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
                            name: "ok".into(),
                            kind: ColumnKind::Bool,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![Interaction::AssertVisibleFresh {
                table: 0,
                row: SimRow {
                    values: vec![SimValue::U64(7), SimValue::Bool(true)],
                },
            }],
        };
        let failure = run_case_detailed(&case).expect_err("case should fail");
        let artifact = DatastoreBugArtifact {
            seed: case.seed.0,
            failure,
            case: case.clone(),
            shrunk_case: Some(case),
        };

        save_bug_artifact(&path, &artifact).expect("save artifact");
        let loaded = load_bug_artifact(&path).expect("load artifact");
        assert_eq!(loaded, artifact);
    }
}
