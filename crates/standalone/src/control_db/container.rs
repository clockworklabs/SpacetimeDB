//! The container attached to each database, for servers that run containers.
//!
//! A database has at most one container. Its `generation` names the one instance
//! allowed to run, and only that generation's credentials are valid. With a single
//! node there is no separate claim step: creating a container, replacing a running
//! container's spec, starting it (even when it already runs), and resetting its
//! database while it runs all take a new generation and clear the reported status.
//! Generations are never reused for a database identity, including across removal
//! and re-creation of the container or of the database.
use super::environment::transaction_error;
use super::*;
use spacetimedb::auth::identity::ContainerClaim;
use spacetimedb_lib::container::{ContainerInfo, ContainerSpec, ContainerState};

const CONTAINER_TREE: &str = "container";
const STATUS_TREE: &str = "container_status";
/// The last generation of a removed container, by database identity, so that it
/// outlives the database.
const GENERATION_TREE: &str = "container_generation";

#[derive(serde::Serialize, serde::Deserialize)]
struct Container {
    spec: ContainerSpec,
    running: bool,
    generation: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Status {
    generation: u64,
    state: ContainerState,
}

fn decode<T: serde::de::DeserializeOwned>(value: Option<sled::IVec>) -> Result<Option<T>> {
    Ok(value.map(|value| serde_json::from_slice(&value)).transpose()?)
}

fn get<T: serde::de::DeserializeOwned>(
    tree: &TransactionalTree,
    key: &[u8],
) -> ConflictableTransactionResult<Option<T>, Error> {
    decode(tree.get(key)?).map_err(ConflictableTransactionError::Abort)
}

fn put(
    tree: &TransactionalTree,
    key: &[u8],
    value: &impl serde::Serialize,
) -> ConflictableTransactionResult<(), Error> {
    let value = serde_json::to_vec(value).map_err(|e| ConflictableTransactionError::Abort(e.into()))?;
    tree.insert(key, value)?;
    Ok(())
}

impl ControlDb {
    /// The container of a database, with the status reported for its current generation.
    pub fn get_container(&self, database_id: u64) -> Result<Option<ContainerInfo>> {
        let key = database_id.to_be_bytes();
        let Some(container) = decode::<Container>(self.db.open_tree(CONTAINER_TREE)?.get(key)?)? else {
            return Ok(None);
        };
        let status = decode::<Status>(self.db.open_tree(STATUS_TREE)?.get(key)?)?;
        Ok(Some(ContainerInfo {
            spec: container.spec,
            running: container.running,
            generation: container.generation,
            state: status
                .filter(|status| status.generation == container.generation)
                .map(|status| status.state),
        }))
    }

    /// Whether `claim` names the current generation of a running container.
    pub fn is_current_container(&self, claim: &ContainerClaim) -> Result<bool> {
        let Some(database) = self.get_database_by_identity(&claim.database)? else {
            return Ok(false);
        };
        let container = self.db.open_tree(CONTAINER_TREE)?.get(database.id.to_be_bytes())?;
        Ok(decode::<Container>(container)?.is_some_and(|c| c.running && c.generation == claim.generation))
    }

    /// Set, replace, or remove (`None`) the container of a database.
    /// A new container starts running.
    pub fn set_container(
        &self,
        database_id: u64,
        database_identity: &Identity,
        spec: Option<ContainerSpec>,
    ) -> Result<()> {
        let identity = database_identity.to_be_byte_array();
        self.update_container(database_id, |containers, statuses, generations, key| {
            match (get::<Container>(containers, key)?, spec.clone()) {
                (Some(mut container), Some(spec)) => {
                    // A running container restarts with the new spec.
                    container.generation += u64::from(container.running);
                    container.spec = spec;
                    put(containers, key, &container)?;
                }
                (None, Some(spec)) => {
                    let generation = get::<u64>(generations, &identity)?.unwrap_or(0) + 1;
                    let container = Container {
                        spec,
                        running: true,
                        generation,
                    };
                    put(containers, key, &container)?;
                }
                (Some(container), None) => {
                    put(generations, &identity, &container.generation)?;
                    containers.remove(key)?;
                }
                (None, None) => {}
            }
            // Any reported status belongs to an earlier generation.
            statuses.remove(key)?;
            Ok(())
        })
    }

    /// Start or stop the container of a database. Starting always takes a new generation,
    /// which restarts a running container. Returns `false` if the database has no container.
    pub fn set_container_running(&self, database_id: u64, running: bool) -> Result<bool> {
        self.update_container(database_id, |containers, statuses, _, key| {
            let Some(mut container) = get::<Container>(containers, key)? else {
                return Ok(false);
            };
            container.generation += u64::from(running);
            container.running = running;
            put(containers, key, &container)?;
            statuses.remove(key)?;
            Ok(true)
        })
    }

    /// Restart the container of a reset database under a new generation, if it is running.
    pub fn restart_container(&self, database_id: u64) -> Result<()> {
        self.update_container(database_id, |containers, statuses, _, key| {
            if let Some(mut container) = get::<Container>(containers, key)?
                && container.running
            {
                container.generation += 1;
                put(containers, key, &container)?;
                statuses.remove(key)?;
            }
            Ok(())
        })
    }

    /// Atomically update the container records of a database, then flush them.
    fn update_container<T>(
        &self,
        database_id: u64,
        update: impl Fn(
            &TransactionalTree,
            &TransactionalTree,
            &TransactionalTree,
            &[u8],
        ) -> ConflictableTransactionResult<T, Error>,
    ) -> Result<T> {
        let containers = self.db.open_tree(CONTAINER_TREE)?;
        let statuses = self.db.open_tree(STATUS_TREE)?;
        let generations = self.db.open_tree(GENERATION_TREE)?;
        let key = database_id.to_be_bytes();
        let result: TransactionResult<T, Error> = (&containers, &statuses, &generations)
            .transaction(|(containers, statuses, generations)| update(containers, statuses, generations, &key));
        let result = result.map_err(transaction_error)?;
        self.db.flush()?;
        Ok(result)
    }
}
