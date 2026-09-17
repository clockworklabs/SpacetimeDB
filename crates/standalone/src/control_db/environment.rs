//! Private initialization inputs, separate from the historical public Database encoding.
//!
//! Creation and reset atomically persist both database indexes, the generation
//! and initial-program binding, the complete ENV input, and the nominated leader.
//! The transaction is flushed before host launch, so a lost request response can
//! recover the same generation through ordinary leader lookup.
//!
//! Reading the input rechecks the persisted owner, program, and generation in
//! the same transaction. A legacy generation with neither metadata nor input has
//! an empty environment; missing input for an active generation is an error.
//! A reset performed by a pre-ENV server replaces the replica without updating
//! these trees. Records belonging to that removed replica are ignored.
//!
//! The host reads this input only before the database's first initialization.
//! Reopening an initialized database uses its committed program and `st_env`,
//! including later module updates. The initial input is no longer needed once
//! initialization is durable, but is currently retained until reset replaces it
//! or database deletion removes it atomically with both indexes and the binding.
use super::*;
use spacetimedb_client_api_messages::publish::PublishRequest;
use spacetimedb_lib::Hash;
use std::collections::BTreeMap;

// Keep the existing tree name for on-disk compatibility.
const METADATA_TREE: &str = "database_bootstrap";
const VALUES_TREE: &str = "initial_environment";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializationMetadata {
    version: u8,
    database_id: u64,
    identity: Identity,
    program: Hash,
    generation: u64,
    // Earlier ENV development builds did not record the replica ID.
    #[serde(default)]
    replica_id: Option<u64>,
}

fn invalid() -> Error {
    Error::Other(anyhow::anyhow!("invalid private initialization record"))
}

impl InitializationMetadata {
    fn decode(bytes: &[u8], database: &Database) -> Result<Self> {
        if bytes.len() > 1024 {
            return Err(invalid());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if value.version != 1
            || value.database_id != database.id
            || value.identity != database.database_identity
            || value.generation == 0
        {
            return Err(invalid());
        }
        Ok(value)
    }

    fn current_generation(&self, database: &Database, replica_exists: bool) -> Result<u64> {
        // A pre-ENV server leaves these records behind when resetting a database,
        // but replaces its replica. Never reuse that replica's initial values.
        if !replica_exists {
            return Ok(0);
        }
        if self.program != database.initial_program {
            return Err(invalid());
        }
        Ok(self.generation)
    }
}

impl ControlDb {
    pub(crate) fn with_initialization_generation(&self, mut database: Database) -> Result<Database> {
        if let Some(bytes) = self.db.open_tree(METADATA_TREE)?.get(database.id.to_be_bytes())? {
            let binding = InitializationMetadata::decode(&bytes, &database)?;
            let replica_exists = match binding.replica_id {
                Some(id) => self.db.open_tree("replica")?.contains_key(id.to_be_bytes())?,
                None => true,
            };
            database.bootstrap_generation = binding.current_generation(&database, replica_exists)?;
        }
        Ok(database)
    }

    /// Recheck the exact persisted generation and program before releasing any values.
    pub(crate) fn initial_environment(&self, database: &Database, replica_id: u64) -> Result<BTreeMap<String, String>> {
        let databases = self.db.open_tree("database_by_identity")?;
        let metadata = self.db.open_tree(METADATA_TREE)?;
        let values = self.db.open_tree(VALUES_TREE)?;
        let replicas = self.db.open_tree("replica")?;
        let result: TransactionResult<Option<sled::IVec>, Error> = (&databases, &metadata, &values, &replicas)
            .transaction(|(databases, metadata, values, replicas)| {
                let persisted = databases
                    .get(database.database_identity.to_be_byte_array())?
                    .ok_or_else(|| {
                        ConflictableTransactionError::Abort(Error::DatabaseNotFound(database.database_identity))
                    })?;
                let stored: Database = compat::Database::from_slice(&persisted)
                    .map_err(|_| ConflictableTransactionError::Abort(invalid()))?
                    .into();
                if stored.id != database.id
                    || stored.initial_program != database.initial_program
                    || stored.owner_identity != database.owner_identity
                {
                    return transaction::abort(invalid());
                }
                match metadata.get(database.id.to_be_bytes())? {
                    Some(bytes) => {
                        // A reset can replace the selected replica before launch.
                        // Check its nomination in the same snapshot as the generation and values.
                        let replica = replicas
                            .get(replica_id.to_be_bytes())?
                            .ok_or_else(|| ConflictableTransactionError::Abort(invalid()))?;
                        let replica: Replica =
                            bsatn::from_slice(&replica).map_err(|_| ConflictableTransactionError::Abort(invalid()))?;
                        if replica.database_id != database.id {
                            return transaction::abort(invalid());
                        }
                        let binding = InitializationMetadata::decode(&bytes, &stored)
                            .map_err(ConflictableTransactionError::Abort)?;
                        let replica_exists = match binding.replica_id {
                            Some(id) => replicas.get(id.to_be_bytes())?.is_some(),
                            None => true,
                        };
                        let generation = binding
                            .current_generation(&stored, replica_exists)
                            .map_err(ConflictableTransactionError::Abort)?;
                        if generation != database.bootstrap_generation {
                            return transaction::abort(invalid());
                        }
                        if generation == 0 {
                            return Ok(None);
                        }
                        let bytes = values
                            .get(database.id.to_be_bytes())?
                            .ok_or_else(|| ConflictableTransactionError::Abort(invalid()))?;
                        Ok(Some(bytes))
                    }
                    None if database.bootstrap_generation == 0 => {
                        if values.get(database.id.to_be_bytes())?.is_some() {
                            return transaction::abort(invalid());
                        }
                        Ok(None)
                    }
                    None => transaction::abort(invalid()),
                }
            });
        let bytes = result.map_err(transaction_error)?;
        match bytes {
            None => Ok(BTreeMap::new()),
            Some(bytes) => {
                let request = PublishRequest::decode(&bytes).map_err(|_| invalid())?;
                if request.module.as_ref().is_some_and(|module| !module.is_empty()) {
                    return Err(invalid());
                }
                Ok(request.environment)
            }
        }
    }

    /// Atomically write the database record, initial environment, and new replica.
    /// Remove any supplied previous replica records in the same transaction.
    ///
    /// With `expected = None`, require that the database does not exist.
    /// With `expected = Some(previous)`, require that it still matches `previous`.
    ///
    /// Does not start or stop replicas.
    pub(crate) fn upsert_database_with_environment(
        &self,
        mut database: Database,
        expected: Option<&Database>,
        environment: BTreeMap<String, String>,
        previous_replicas: &[Replica],
    ) -> Result<(Database, Replica)> {
        if expected.is_none() {
            database.id = self.db.generate_id()?;
        }
        database.bootstrap_generation = expected
            .map_or(Some(1), |old| old.bootstrap_generation.checked_add(1))
            .ok_or_else(invalid)?;
        let replica = Replica {
            id: self.db.generate_id()?,
            database_id: database.id,
            node_id: 0,
            leader: true,
        };
        let input = PublishRequest {
            module: None,
            environment,
            ..Default::default()
        }
        .encode()
        .map_err(|_| invalid())?;
        let binding = InitializationMetadata {
            version: 1,
            database_id: database.id,
            identity: database.database_identity,
            program: database.initial_program,
            generation: database.bootstrap_generation,
            replica_id: Some(replica.id),
        };
        let binding = serde_json::to_vec(&binding).map_err(|_| invalid())?;
        let encoded = compat::Database::from(database.clone()).to_vec()?;
        let encoded_replica = bsatn::to_vec(&replica)?;
        let databases = self.db.open_tree("database")?;
        let identities = self.db.open_tree("database_by_identity")?;
        let metadata = self.db.open_tree(METADATA_TREE)?;
        let values = self.db.open_tree(VALUES_TREE)?;
        let replicas = self.db.open_tree("replica")?;
        let result: TransactionResult<(), Error> = (&databases, &identities, &metadata, &values, &replicas)
            .transaction(|(databases, identities, metadata, values, replicas)| {
                let identity_key = database.database_identity.to_be_byte_array();
                match (expected, identities.get(identity_key)?) {
                    (None, None) => {}
                    (Some(expected), Some(bytes)) => {
                        let stored: Database = compat::Database::from_slice(&bytes)
                            .map_err(|_| ConflictableTransactionError::Abort(invalid()))?
                            .into();
                        if stored.id != expected.id
                            || stored.initial_program != expected.initial_program
                            || stored.owner_identity != expected.owner_identity
                        {
                            return transaction::abort(invalid());
                        }
                        let generation = match metadata.get(stored.id.to_be_bytes())? {
                            Some(bytes) => {
                                let binding = InitializationMetadata::decode(&bytes, &stored)
                                    .map_err(ConflictableTransactionError::Abort)?;
                                let replica_exists = match binding.replica_id {
                                    Some(id) => replicas.get(id.to_be_bytes())?.is_some(),
                                    None => true,
                                };
                                binding
                                    .current_generation(&stored, replica_exists)
                                    .map_err(ConflictableTransactionError::Abort)?
                            }
                            None => 0,
                        };
                        if generation != expected.bootstrap_generation {
                            return transaction::abort(invalid());
                        }
                    }
                    (None, Some(_)) => {
                        return transaction::abort(Error::DatabaseAlreadyExists(database.database_identity))
                    }
                    (Some(_), None) => return transaction::abort(Error::DatabaseNotFound(database.database_identity)),
                }
                databases.insert(&database.id.to_be_bytes(), encoded.clone())?;
                identities.insert(&identity_key, encoded.clone())?;
                metadata.insert(&database.id.to_be_bytes(), binding.clone())?;
                values.insert(&database.id.to_be_bytes(), input.clone())?;
                for previous in previous_replicas {
                    if previous.database_id != database.id {
                        return transaction::abort(invalid());
                    }
                    replicas.remove(&previous.id.to_be_bytes())?;
                }
                replicas.insert(&replica.id.to_be_bytes(), encoded_replica.clone())?;
                databases.flush();
                Ok(())
            });
        result.map_err(transaction_error)?;
        self.db.flush()?;
        Ok((database, replica))
    }

    pub(super) fn delete_database_and_environment(&self, id: u64) -> Result<Option<u64>> {
        let databases = self.db.open_tree("database")?;
        let identities = self.db.open_tree("database_by_identity")?;
        let metadata = self.db.open_tree(METADATA_TREE)?;
        let values = self.db.open_tree(VALUES_TREE)?;
        let result: TransactionResult<Option<u64>, Error> =
            (&databases, &identities, &metadata, &values).transaction(|(databases, identities, metadata, values)| {
                let Some(bytes) = databases.get(id.to_be_bytes())? else {
                    return Ok(None);
                };
                let database =
                    compat::Database::from_slice(&bytes).map_err(|_| ConflictableTransactionError::Abort(invalid()))?;
                identities.remove(&database.database_identity().to_be_byte_array())?;
                databases.remove(&id.to_be_bytes())?;
                metadata.remove(&id.to_be_bytes())?;
                values.remove(&id.to_be_bytes())?;
                databases.flush();
                Ok(Some(id))
            });
        let result = result.map_err(transaction_error)?;
        self.db.flush()?;
        Ok(result)
    }
}

fn transaction_error(error: TransactionError<Error>) -> Error {
    match error {
        TransactionError::Abort(error) => error,
        TransactionError::Storage(error) => error.into(),
    }
}

#[async_trait::async_trait]
impl spacetimedb::host::InitialEnvironmentSource for ControlDb {
    async fn load(&self, database: &Database, replica_id: u64) -> anyhow::Result<BTreeMap<String, String>> {
        let source = self.clone();
        let database = database.clone();
        spacetimedb::util::asyncify(move || {
            let database = source.with_initialization_generation(database)?;
            source.initial_environment(&database, replica_id)
        })
        .await
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb::messages::control_db::HostType;

    fn database() -> Database {
        Database {
            id: 0,
            database_identity: Identity::ZERO,
            owner_identity: Identity::ZERO,
            host_type: HostType::Wasm,
            initial_program: spacetimedb_lib::hash_bytes(b"module"),
            bootstrap_generation: 0,
        }
    }

    #[test]
    fn initialization_is_durable_atomic_generation_bound_and_deleted_with_database() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let (original, original_replica) = {
            let control = ControlDb::at(temp.path())?;
            control.upsert_database_with_environment(
                database(),
                None,
                BTreeMap::from([("VALUE".into(), "first".into())]),
                &[],
            )?
        };
        let control = ControlDb::at(temp.path())?;
        let loaded = control.with_initialization_generation(control.get_database_by_id(original.id)?.unwrap())?;
        assert_eq!(loaded.bootstrap_generation, 1);
        assert_eq!(
            control.initial_environment(&loaded, original_replica.id)?["VALUE"],
            "first"
        );
        assert_eq!(
            control.get_leader_replica_by_database(loaded.id).unwrap().id,
            original_replica.id
        );
        let (replaced, new_replica) = control.upsert_database_with_environment(
            loaded.clone(),
            Some(&loaded),
            BTreeMap::new(),
            std::slice::from_ref(&original_replica),
        )?;
        assert_eq!(replaced.bootstrap_generation, 2);
        assert!(control.initial_environment(&original, original_replica.id).is_err());
        assert!(control.initial_environment(&replaced, new_replica.id)?.is_empty());
        // A lookup can select the old replica, then read the new generation after reset.
        assert!(control.initial_environment(&replaced, original_replica.id).is_err());
        assert_eq!(control.get_replicas_by_database(loaded.id)?.len(), 1);
        assert_eq!(
            control.get_leader_replica_by_database(loaded.id).unwrap().id,
            new_replica.id
        );
        assert!(control
            .upsert_database_with_environment(loaded.clone(), Some(&loaded), BTreeMap::new(), &[])
            .is_err());
        assert_eq!(
            control
                .with_initialization_generation(control.get_database_by_id(loaded.id)?.unwrap())?
                .bootstrap_generation,
            2
        );
        control.delete_database(loaded.id)?;
        assert!(control.initial_environment(&replaced, new_replica.id).is_err());
        assert!(control.db.open_tree(VALUES_TREE)?.is_empty());
        Ok(())
    }

    #[test]
    fn corrupt_initialization_does_not_prevent_listing_or_deletion() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let control = ControlDb::at(temp.path())?;
        let (database, replica) = control.upsert_database_with_environment(database(), None, BTreeMap::new(), &[])?;
        control
            .db
            .open_tree(METADATA_TREE)?
            .insert(database.id.to_be_bytes(), b"corrupt")?;
        assert!(control.get_database_by_id(database.id)?.is_some());
        assert!(control.get_database_by_identity(&database.database_identity)?.is_some());
        assert_eq!(control.get_databases()?.len(), 1);
        assert!(control.initial_environment(&database, replica.id).is_err());
        control.delete_database(database.id)?;
        assert!(control.get_databases()?.is_empty());
        assert!(control.db.open_tree(METADATA_TREE)?.is_empty());
        assert!(control.db.open_tree(VALUES_TREE)?.is_empty());
        Ok(())
    }

    #[test]
    fn legacy_absence_is_empty_but_missing_new_values_fails_closed() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let control = ControlDb::at(temp.path())?;
        let id = control.insert_database(database())?;
        let legacy = control.get_database_by_id(id)?.unwrap();
        assert!(control.initial_environment(&legacy, 0)?.is_empty());
        let (new, replica) =
            control.upsert_database_with_environment(legacy.clone(), Some(&legacy), BTreeMap::new(), &[])?;
        control.db.open_tree(VALUES_TREE)?.remove(new.id.to_be_bytes())?;
        assert!(control.initial_environment(&new, replica.id).is_err());
        Ok(())
    }

    #[test]
    fn legacy_reset_round_trip_does_not_reuse_initial_environment_values() -> anyhow::Result<()> {
        use std::result::Result;

        // Freeze the pre-ENV on-disk database format independently of `compat`.
        #[derive(spacetimedb_lib::ser::Serialize, spacetimedb_lib::de::Deserialize)]
        struct LegacyDatabase {
            id: u64,
            database_identity: Identity,
            owner_identity: Identity,
            host_type: HostType,
            initial_program: Hash,
        }

        for changed_program in [false, true] {
            for values in [BTreeMap::new(), BTreeMap::from([("SECRET".into(), "old-value".into())])] {
                let temp = tempfile::tempdir()?;
                let (original, old_replica) = {
                    let control = ControlDb::at(temp.path())?;
                    control.upsert_database_with_environment(database(), None, values, &[])?
                };
                let replacement_id = {
                    // Reopen using only the old format and old reset operations:
                    // update both database indexes, remove the old replica, and
                    // insert a fresh replica. The ENV trees are untouched.
                    let db = sled::open(temp.path())?;
                    let identities = db.open_tree("database_by_identity")?;
                    let identity = original.database_identity.to_be_byte_array();
                    let mut old: LegacyDatabase = bsatn::from_slice(&identities.get(identity)?.unwrap())?;
                    if changed_program {
                        old.initial_program = spacetimedb_lib::hash_bytes(b"replacement module");
                    }
                    let bytes = bsatn::to_vec(&old)?;
                    identities.insert(identity, bytes.clone())?;
                    db.open_tree("database")?.insert(old.id.to_be_bytes(), bytes)?;
                    let replicas = db.open_tree("replica")?;
                    replicas.remove(old_replica.id.to_be_bytes())?;
                    let id = db.generate_id()?;
                    let replacement = Replica {
                        id,
                        ..old_replica.clone()
                    };
                    replicas.insert(id.to_be_bytes(), bsatn::to_vec(&replacement)?)?;
                    db.flush()?;
                    id
                };
                let control = ControlDb::at(temp.path())?;
                let loaded =
                    control.with_initialization_generation(control.get_database_by_id(original.id)?.unwrap())?;
                assert_eq!(loaded.bootstrap_generation, 0);
                assert!(control.initial_environment(&loaded, replacement_id)?.is_empty());
                assert!(control.initial_environment(&loaded, old_replica.id).is_err());

                // A subsequent new-version reset must succeed and use only its
                // freshly supplied values, even if the old reset never ran init.
                let previous = control.get_replicas_by_database(loaded.id)?;
                let (new, replica) = control.upsert_database_with_environment(
                    loaded.clone(),
                    Some(&loaded),
                    BTreeMap::from([("SECRET".into(), "fresh-value".into())]),
                    &previous,
                )?;
                assert_eq!(control.initial_environment(&new, replica.id)?["SECRET"], "fresh-value");
            }
        }
        Ok(())
    }

    #[test]
    fn active_initialization_still_rejects_a_program_mismatch() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let control = ControlDb::at(temp.path())?;
        let (database, replica) = control.upsert_database_with_environment(database(), None, BTreeMap::new(), &[])?;
        let tree = control.db.open_tree(METADATA_TREE)?;
        let mut binding = InitializationMetadata::decode(&tree.get(database.id.to_be_bytes())?.unwrap(), &database)?;
        binding.program = spacetimedb_lib::hash_bytes(b"incorrect program");
        tree.insert(database.id.to_be_bytes(), serde_json::to_vec(&binding)?)?;
        assert!(control.with_initialization_generation(database.clone()).is_err());
        assert!(control.initial_environment(&database, replica.id).is_err());
        Ok(())
    }
}
