//! Private bootstrap inputs, separate from the historical public Database encoding.
//!
//! Creation and reset atomically persist both database indexes, the generation
//! and initial-program binding, the complete ENV input, and the nominated leader.
//! The transaction is flushed before host launch, so a lost request response can
//! recover the same generation through ordinary leader lookup.
//!
//! Reading the input rechecks the persisted owner, program, and generation in
//! the same transaction. A legacy generation with neither metadata nor input has
//! an empty environment; missing input for a recorded generation is an error.
//!
//! The host reads this input only before the database's first initialization.
//! Reopening an initialized database uses its committed program and `st_env`,
//! including later module updates. Reset replaces the bootstrap input, and
//! database deletion removes it atomically with both indexes and the binding.
use super::*;
use spacetimedb_client_api_messages::publish::PublishRequest;
use spacetimedb_lib::Hash;
use std::collections::BTreeMap;

const METADATA_TREE: &str = "database_bootstrap";
const VALUES_TREE: &str = "initial_environment";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    version: u8,
    database_id: u64,
    identity: Identity,
    program: Hash,
    generation: u64,
}

fn invalid() -> Error {
    Error::Other(anyhow::anyhow!("invalid private bootstrap record"))
}

impl Bootstrap {
    fn decode(bytes: &[u8], database: &Database) -> Result<Self> {
        if bytes.len() > 1024 {
            return Err(invalid());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if value.version != 1
            || value.database_id != database.id
            || value.identity != database.database_identity
            || value.program != database.initial_program
            || value.generation == 0
        {
            return Err(invalid());
        }
        Ok(value)
    }
}

impl ControlDb {
    pub(super) fn decode_database(&self, bytes: &[u8]) -> Result<Database> {
        let mut database: Database = compat::Database::from_slice(bytes)?.into();
        if let Some(bytes) = self.db.open_tree(METADATA_TREE)?.get(database.id.to_be_bytes())? {
            database.bootstrap_generation = Bootstrap::decode(&bytes, &database)?.generation;
        }
        Ok(database)
    }

    /// Recheck the exact persisted generation and program before releasing any values.
    pub(crate) fn initial_environment(&self, database: &Database) -> Result<BTreeMap<String, String>> {
        let databases = self.db.open_tree("database_by_identity")?;
        let metadata = self.db.open_tree(METADATA_TREE)?;
        let values = self.db.open_tree(VALUES_TREE)?;
        let result: TransactionResult<Option<sled::IVec>, Error> =
            (&databases, &metadata, &values).transaction(|(databases, metadata, values)| {
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
                        let binding =
                            Bootstrap::decode(&bytes, &stored).map_err(ConflictableTransactionError::Abort)?;
                        if binding.generation != database.bootstrap_generation {
                            return transaction::abort(invalid());
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

    /// Install the desired initial database, its complete private input and a
    /// durable leader nomination together. Cancellation before host launch can
    /// therefore recover through the ordinary leader lookup.
    pub(crate) fn install_database_with_environment(
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
        let binding = Bootstrap {
            version: 1,
            database_id: database.id,
            identity: database.database_identity,
            program: database.initial_program,
            generation: database.bootstrap_generation,
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
                                Bootstrap::decode(&bytes, &stored)
                                    .map_err(ConflictableTransactionError::Abort)?
                                    .generation
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
    async fn load(&self, database: &Database) -> anyhow::Result<BTreeMap<String, String>> {
        let source = self.clone();
        let database = database.clone();
        spacetimedb::util::asyncify(move || source.initial_environment(&database))
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
    fn bootstrap_is_durable_atomic_generation_bound_and_deleted_with_database() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let (original, original_replica) = {
            let control = ControlDb::at(temp.path())?;
            control.install_database_with_environment(
                database(),
                None,
                BTreeMap::from([("VALUE".into(), "first".into())]),
                &[],
            )?
        };
        let control = ControlDb::at(temp.path())?;
        let loaded = control.get_database_by_id(original.id)?.unwrap();
        assert_eq!(loaded.bootstrap_generation, 1);
        assert_eq!(control.initial_environment(&loaded)?["VALUE"], "first");
        assert_eq!(
            control.get_leader_replica_by_database(loaded.id).unwrap().id,
            original_replica.id
        );
        let (replaced, new_replica) = control.install_database_with_environment(
            loaded.clone(),
            Some(&loaded),
            BTreeMap::new(),
            &[original_replica],
        )?;
        assert_eq!(replaced.bootstrap_generation, 2);
        assert!(control.initial_environment(&original).is_err());
        assert!(control.initial_environment(&replaced)?.is_empty());
        assert_eq!(control.get_replicas_by_database(loaded.id)?.len(), 1);
        assert_eq!(
            control.get_leader_replica_by_database(loaded.id).unwrap().id,
            new_replica.id
        );
        assert!(control
            .install_database_with_environment(loaded.clone(), Some(&loaded), BTreeMap::new(), &[])
            .is_err());
        assert_eq!(control.get_database_by_id(loaded.id)?.unwrap().bootstrap_generation, 2);
        control.delete_database(loaded.id)?;
        assert!(control.initial_environment(&replaced).is_err());
        assert!(control.db.open_tree(VALUES_TREE)?.is_empty());
        Ok(())
    }

    #[test]
    fn legacy_absence_is_empty_but_missing_new_values_fails_closed() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let control = ControlDb::at(temp.path())?;
        let id = control.insert_database(database())?;
        let legacy = control.get_database_by_id(id)?.unwrap();
        assert!(control.initial_environment(&legacy)?.is_empty());
        let (new, _) =
            control.install_database_with_environment(legacy.clone(), Some(&legacy), BTreeMap::new(), &[])?;
        control.db.open_tree(VALUES_TREE)?.remove(new.id.to_be_bytes())?;
        assert!(control.initial_environment(&new).is_err());
        Ok(())
    }
}
