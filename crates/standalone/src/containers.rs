//! What the container supervisor reads from and reports to a standalone server.
//!
//! A standalone server is a single node, which runs every running container. Setting or
//! starting a container assigns it a new generation right away, so there is nothing to claim.

use std::collections::HashMap;

use async_trait::async_trait;
use spacetimedb_container_supervisor::{is_finished, Assigned, Assignments, ContainerControl};
use spacetimedb_lib::container::ContainerState;

use crate::StandaloneEnv;

#[async_trait]
impl ContainerControl for StandaloneEnv {
    async fn node_id(&self) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn assignments(&self, _node_id: u64) -> anyhow::Result<Assignments> {
        let mut assigned = HashMap::new();
        for (database, container) in self.control_db.running_containers()? {
            let replicas = self.control_db.get_replicas_by_database(database.id)?;
            let leader_replica = replicas.iter().find(|replica| replica.leader).map(|replica| replica.id);
            let finished = container
                .state
                .as_ref()
                .is_some_and(|state| is_finished(container.spec.restart, state));
            assigned.insert(
                database.id,
                Assigned {
                    database_identity: database.database_identity,
                    generation: container.generation,
                    spec: container.spec,
                    leader_replica,
                    finished,
                },
            );
        }
        Ok(Assignments {
            claims: Vec::new(),
            assigned,
        })
    }

    async fn claim(&self, _database_id: u64) -> anyhow::Result<()> {
        Ok(())
    }

    async fn report(&self, database_id: u64, generation: u64, state: ContainerState) -> anyhow::Result<()> {
        Ok(self.control_db.set_container_status(database_id, generation, state)?)
    }
}
