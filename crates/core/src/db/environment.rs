//! Dedicated access to the private environment store.
//!
//! Publishing replaces the complete environment inside the program transaction.
//! Helpers never acquire a second transaction or expose individual mutation APIs.

use super::relational_db::{MutTx, RelationalDB};
use crate::error::DBError;
use spacetimedb_datastore::error::DatastoreError;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{StEnvFields, StEnvRow, ST_ENV_ID};
use spacetimedb_lib::environment::{
    validate_key, EnvironmentSchema, EnvironmentSchemaError, EnvironmentValidationError,
};
use spacetimedb_sats::AlgebraicValue;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error(transparent)]
    Validation(#[from] EnvironmentValidationError),
    #[error(transparent)]
    Schema(#[from] EnvironmentSchemaError),
    #[error(transparent)]
    Datastore(#[from] DatastoreError),
    #[error(transparent)]
    Database(#[from] DBError),
}

/// Read from exactly the caller's snapshot, preserving missing versus empty.
pub fn get(state: &impl StateView, key: &str) -> Result<Option<String>, EnvironmentError> {
    validate_key(key)?;
    state
        .iter_by_col_eq(ST_ENV_ID, StEnvFields::Key, &AlgebraicValue::String(key.into()))?
        .next()
        .map(|row| Ok(StEnvRow::try_from(row)?.value))
        .transpose()
}

pub fn snapshot(state: &impl StateView) -> Result<BTreeMap<String, String>, EnvironmentError> {
    state
        .iter(ST_ENV_ID)?
        .map(|row| {
            let row = StEnvRow::try_from(row)?;
            Ok((row.key, row.value))
        })
        .collect()
}

/// Apply a complete publish configuration in the caller's program transaction.
/// Validate every input before modifying any row, even if the caller chooses to
/// recover from a validation error and commit other transaction work.
pub fn replace(
    db: &RelationalDB,
    tx: &mut MutTx,
    schema: &EnvironmentSchema,
    values: &BTreeMap<String, String>,
) -> Result<(), EnvironmentError> {
    schema.validate_values(values)?;
    let previous = snapshot(tx)?;
    for (key, value) in &previous {
        if values.get(key) != Some(value) {
            delete(db, tx, key)?;
        }
    }
    for (key, value) in values {
        if previous.get(key) != Some(value) {
            tx.insert_via_serialize_bsatn(
                ST_ENV_ID,
                &StEnvRow {
                    key: key.clone(),
                    value: value.clone(),
                },
            )?;
        }
    }
    Ok(())
}

fn delete(db: &RelationalDB, tx: &mut MutTx, key: &str) -> Result<bool, EnvironmentError> {
    validate_key(key)?;
    let pointer = tx
        .iter_by_col_eq(ST_ENV_ID, StEnvFields::Key, &AlgebraicValue::String(key.into()))?
        .next()
        .map(|row| row.pointer());
    if let Some(pointer) = pointer {
        db.delete(tx, ST_ENV_ID, [pointer]);
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::TestDB;
    use spacetimedb_datastore::execution_context::Workload;
    use spacetimedb_lib::environment::{EnvironmentConstraint, EnvironmentDeclaration};

    fn schema() -> EnvironmentSchema {
        EnvironmentSchema::new(vec![
            EnvironmentDeclaration {
                name: "REQUIRED".into(),
                constraint: EnvironmentConstraint::AnyString,
                optional: false,
            },
            EnvironmentDeclaration {
                name: "OPTIONAL".into(),
                constraint: EnvironmentConstraint::AnyString,
                optional: true,
            },
        ])
        .unwrap()
    }

    #[test]
    fn replacement_preserves_empty_and_nul_and_removes_omitted_values() {
        let db = TestDB::in_memory().unwrap();
        let initial = BTreeMap::from([("REQUIRED".into(), "".into()), ("OPTIONAL".into(), "a\0b".into())]);
        db.with_auto_commit(Workload::ForTests, |tx| replace(&db, tx, &schema(), &initial))
            .unwrap();
        db.with_read_only(Workload::ForTests, |tx| assert_eq!(snapshot(tx).unwrap(), initial));
        let next = BTreeMap::from([("REQUIRED".into(), "new".into())]);
        db.with_auto_commit(Workload::ForTests, |tx| replace(&db, tx, &schema(), &next))
            .unwrap();
        db.with_read_only(Workload::ForTests, |tx| assert_eq!(snapshot(tx).unwrap(), next));
        db.with_auto_commit(Workload::ForTests, |tx| {
            replace(&db, tx, &EnvironmentSchema::default(), &BTreeMap::new())
        })
        .unwrap();
        db.with_read_only(Workload::ForTests, |tx| assert!(snapshot(tx).unwrap().is_empty()));
    }

    #[test]
    fn invalid_complete_input_does_not_reuse_stored_values_or_mutate() {
        let db = TestDB::in_memory().unwrap();
        let initial = BTreeMap::from([("REQUIRED".into(), "old".into()), ("OPTIONAL".into(), "keep".into())]);
        db.with_auto_commit(Workload::ForTests, |tx| -> Result<(), EnvironmentError> {
            replace(&db, tx, &schema(), &initial)?;
            for invalid in [
                BTreeMap::new(),
                BTreeMap::from([
                    ("REQUIRED".into(), "new".into()),
                    ("INVALID-NAME".into(), "secret-marker".into()),
                ]),
                BTreeMap::from([("REQUIRED".into(), "x".repeat(8193))]),
            ] {
                assert!(replace(&db, tx, &schema(), &invalid).is_err());
                assert_eq!(snapshot(tx)?, initial);
            }
            Ok(())
        })
        .unwrap();
        let failed_publish = db.with_auto_commit(Workload::ForTests, |tx| -> Result<(), EnvironmentError> {
            replace(
                &db,
                tx,
                &schema(),
                &BTreeMap::from([("REQUIRED".into(), "updated".into())]),
            )?;
            // Simulate a later failure in the same publish transaction.
            Err(EnvironmentValidationError::InvalidKey.into())
        });
        assert!(failed_publish.is_err());
        db.with_read_only(Workload::ForTests, |tx| assert_eq!(snapshot(tx).unwrap(), initial));
    }
}
