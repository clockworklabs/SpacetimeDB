//! Dedicated access to the private environment store.
//!
//! Mutation callers must authorize owner/admin access before calling these
//! helpers and commit through the normal module transaction machinery so
//! dependent views refresh. Helpers never acquire a second transaction.

use super::relational_db::{MutTx, RelationalDB};
use crate::error::DBError;
use spacetimedb_datastore::error::DatastoreError;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{StEnvFields, StEnvRow, ST_ENV_ID};
use spacetimedb_lib::environment::{validate_key, validate_value, EnvironmentValidationError, MAX_ENV_VARS};
use spacetimedb_sats::AlgebraicValue;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error(transparent)]
    Validation(#[from] EnvironmentValidationError),
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

/// Insert or replace one key. Validation occurs before any mutation.
pub fn set(db: &RelationalDB, tx: &mut MutTx, key: &str, value: &str) -> Result<(), EnvironmentError> {
    validate_key(key)?;
    validate_value(value)?;
    let previous = get(tx, key)?;
    if previous.is_none() && tx.table_row_count(ST_ENV_ID).unwrap_or(0) >= MAX_ENV_VARS as u64 {
        return Err(EnvironmentValidationError::TooManyVariables.into());
    }
    if previous.as_deref() == Some(value) {
        return Ok(());
    }
    delete(db, tx, key)?;
    tx.insert_via_serialize_bsatn(
        ST_ENV_ID,
        &StEnvRow {
            key: key.into(),
            value: value.into(),
        },
    )?;
    Ok(())
}

pub fn delete(db: &RelationalDB, tx: &mut MutTx, key: &str) -> Result<bool, EnvironmentError> {
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

    #[test]
    fn missing_empty_nul_update_and_rollback() {
        let db = TestDB::in_memory().unwrap();
        db.with_auto_commit(Workload::ForTests, |tx| -> Result<(), EnvironmentError> {
            assert_eq!(get(tx, "EMPTY")?, None);
            set(&db, tx, "EMPTY", "")?;
            set(&db, tx, "NUL", "a\0b")?;
            assert_eq!(get(tx, "EMPTY")?, Some(String::new()));
            assert_eq!(get(tx, "NUL")?, Some("a\0b".into()));
            Ok(())
        })
        .unwrap();
        let result = db.with_auto_commit(Workload::ForTests, |tx| -> Result<(), EnvironmentError> {
            set(&db, tx, "EMPTY", "changed")?;
            delete(&db, tx, "NUL")?;
            Err(EnvironmentValidationError::InvalidKey.into())
        });
        assert!(result.is_err());
        db.with_read_only(Workload::ForTests, |tx| {
            assert_eq!(
                snapshot(tx).unwrap(),
                BTreeMap::from([("EMPTY".into(), "".into()), ("NUL".into(), "a\0b".into())])
            );
        });
    }

    #[test]
    fn capacity_and_value_limits_precede_mutation() {
        let db = TestDB::in_memory().unwrap();
        db.with_auto_commit(Workload::ForTests, |tx| -> Result<(), EnvironmentError> {
            for i in 0..MAX_ENV_VARS {
                set(&db, tx, &format!("K{i}"), "")?;
            }
            assert!(set(&db, tx, "EXTRA", "").is_err());
            set(&db, tx, "K0", "updated")?;
            assert!(set(&db, tx, "K0", &"x".repeat(8193)).is_err());
            assert_eq!(get(tx, "K0")?.as_deref(), Some("updated"));
            assert!(delete(&db, tx, "K1")?);
            assert!(!delete(&db, tx, "MISSING")?);
            set(&db, tx, "EXTRA", "")?;
            Ok(())
        })
        .unwrap();
    }
}
