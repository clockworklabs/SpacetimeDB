use crate::address::Address;
use crate::nodes::control_node::budget_controller;
use crate::nodes::control_node::budget_controller::WorkerBudgetState;
use crate::nodes::control_node::worker_api::worker_connection::WorkerConnectionSender;
use crate::protobuf::control_db::HostType;
use crate::{
    hash::Hash,
    protobuf::{
        control_db::{Database, DatabaseInstance, Node},
        control_worker_api::{
            delete_operation, insert_operation, schedule_update, update_operation, worker_bound_message, BudgetUpdate,
            DeleteOperation, InsertOperation, ScheduleState, ScheduleUpdate, UpdateOperation, WorkerBoundMessage,
        },
    },
};
use prost::Message;
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;

use super::prometheus_metrics::WORKER_NODE_COUNT;
use super::{control_db, worker_api::worker_connection_index::WORKER_CONNECTION_INDEX};

pub async fn create_node(advertise_addr: String) -> Result<u64, anyhow::Error> {
    let node = Node {
        id: 0,
        unschedulable: false,
        advertise_addr,
    };

    let id = control_db::insert_node(node).await?;
    Ok(id)
}

pub async fn node_connected(id: u64) -> Result<(), anyhow::Error> {
    // TODO: change the node status or whatever

    WORKER_NODE_COUNT.inc();
    publish_schedule_state(id).await?;

    // Send initial budget for the node.
    node_publish_budget_state(id).await?;

    Ok(())
}

pub async fn node_disconnected(_id: u64) -> Result<(), anyhow::Error> {
    // TODO: change the node status or whatever

    WORKER_NODE_COUNT.dec();

    Ok(())
}

pub async fn insert_database(
    address: &Address,
    identity: &Hash,
    program_bytes_address: &Hash,
    host_type: HostType,
    num_replicas: u32,
    force: bool,
) -> Result<(), anyhow::Error> {
    let database = Database {
        id: 0,
        address: address.as_slice().to_vec(),
        identity: identity.as_slice().to_owned(),
        host_type: host_type as i32,
        num_replicas,
        program_bytes_address: program_bytes_address.as_slice().to_owned(),
    };

    if force {
        if let Some(database) = control_db::get_database_by_address(address).await? {
            let database_id = database.id;
            schedule_database(None, Some(database)).await?;
            control_db::delete_database(database_id).await?;
            broadcast_schedule_update(ScheduleUpdate {
                r#type: Some(schedule_update::Type::Delete(DeleteOperation {
                    r#type: Some(delete_operation::Type::DatabaseId(database_id)),
                })),
            })
            .await?;
        }
    }

    let mut new_database = database.clone();
    let id = control_db::insert_database(database).await?;
    new_database.id = id;

    broadcast_schedule_update(ScheduleUpdate {
        r#type: Some(schedule_update::Type::Insert(InsertOperation {
            r#type: Some(insert_operation::Type::Database(new_database.clone())),
        })),
    })
    .await?;

    schedule_database(Some(new_database), None).await?;

    Ok(())
}

pub async fn update_database(
    address: &Address,
    program_bytes_address: &Hash,
    num_replicas: u32,
) -> Result<(), anyhow::Error> {
    let database = control_db::get_database_by_address(address).await?;
    let mut database = match database {
        Some(database) => database,
        None => return Ok(()),
    };

    let old_database = database.clone();

    database.program_bytes_address = program_bytes_address.as_slice().to_vec();
    database.num_replicas = num_replicas;
    let new_database = database.clone();
    control_db::update_database(database).await?;

    broadcast_schedule_update(ScheduleUpdate {
        r#type: Some(schedule_update::Type::Update(UpdateOperation {
            r#type: Some(update_operation::Type::Database(new_database.clone())),
        })),
    })
    .await?;

    schedule_database(Some(new_database), Some(old_database)).await?;

    Ok(())
}

pub async fn delete_database(address: &Address) -> Result<(), anyhow::Error> {
    let database = control_db::get_database_by_address(address).await?;
    let database = match database {
        Some(database) => database,
        None => return Ok(()),
    };
    control_db::delete_database(database.id).await?;

    broadcast_schedule_update(ScheduleUpdate {
        r#type: Some(schedule_update::Type::Delete(DeleteOperation {
            r#type: Some(delete_operation::Type::DatabaseId(database.id)),
        })),
    })
    .await?;

    schedule_database(None, Some(database)).await?;

    Ok(())
}

async fn insert_database_instance(database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
    let mut new_database_instance = database_instance.clone();
    let id = control_db::insert_database_instance(database_instance).await?;
    new_database_instance.id = id;

    broadcast_schedule_update(ScheduleUpdate {
        r#type: Some(schedule_update::Type::Insert(InsertOperation {
            r#type: Some(insert_operation::Type::DatabaseInstance(new_database_instance)),
        })),
    })
    .await?;

    Ok(())
}

async fn _update_database_instance(database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
    let new_database_instance = database_instance.clone();
    control_db::_update_database_instance(database_instance).await?;

    broadcast_schedule_update(ScheduleUpdate {
        r#type: Some(schedule_update::Type::Update(UpdateOperation {
            r#type: Some(update_operation::Type::DatabaseInstance(new_database_instance)),
        })),
    })
    .await?;

    Ok(())
}

async fn delete_database_instance(database_instance_id: u64) -> Result<(), anyhow::Error> {
    control_db::delete_database_instance(database_instance_id).await?;

    broadcast_schedule_update(ScheduleUpdate {
        r#type: Some(schedule_update::Type::Delete(DeleteOperation {
            r#type: Some(delete_operation::Type::DatabaseInstanceId(database_instance_id)),
        })),
    })
    .await?;

    Ok(())
}

// Internal
async fn schedule_database(database: Option<Database>, old_database: Option<Database>) -> Result<(), anyhow::Error> {
    let new_replicas = database.as_ref().map(|db| db.num_replicas).unwrap_or(0) as i32;
    let old_replicas = old_database.as_ref().map(|db| db.num_replicas).unwrap_or(0) as i32;
    let replica_diff = new_replicas - old_replicas;

    let database_id = if let Some(database) = database {
        database.id
    } else {
        old_database.unwrap().id
    };

    if replica_diff > 0 {
        schedule_replicas(database_id, replica_diff as u32).await?;
    } else if replica_diff < 0 {
        deschedule_replicas(database_id, replica_diff.abs() as u32).await?;
    }

    Ok(())
}

async fn schedule_replicas(database_id: u64, num_replicas: u32) -> Result<(), anyhow::Error> {
    // Doing some very basic inefficient scheduling
    for i in 0..num_replicas {
        let instances = control_db::get_database_instances().await?;
        let mut histogram: HashMap<u64, u32> = HashMap::new();

        // TODO: filter by live nodes
        let nodes = control_db::get_nodes().await?;
        for node in nodes {
            histogram.insert(node.id, 0);
        }

        for instance in instances {
            let count = if let Some(count) = histogram.get(&instance.node_id) {
                *count
            } else {
                log::warn!("WARNING! You have an instanced scheduled to a node that was never created.");
                continue;
            };
            histogram.insert(instance.node_id, count + 1);
        }

        let mut min_node = 0;
        let mut min_count = u32::MAX;
        for (node_id, count) in histogram {
            if count < min_count {
                min_node = node_id;
                min_count = count;
            }
        }

        let database_instance = DatabaseInstance {
            id: 0,
            database_id,
            node_id: min_node,
            leader: if i == 0 { true } else { false },
        };
        insert_database_instance(database_instance).await?;
    }

    Ok(())
}

async fn deschedule_replicas(database_id: u64, num_replicas: u32) -> Result<(), anyhow::Error> {
    // Delete replicas that are not leaders on the most scheduled nodes
    for _ in 0..num_replicas {
        let instances = control_db::get_database_instances_by_database(database_id).await?;
        let mut histogram: HashMap<u64, u32> = HashMap::new();

        let nodes = control_db::get_nodes().await?;
        for node in nodes {
            histogram.insert(node.id, 0);
        }

        for instance in &instances {
            let count = *histogram.get(&instance.node_id).unwrap();
            histogram.insert(instance.node_id, count + 1);
        }

        let mut max_node = 0;
        let mut max_count = 0;
        for (node_id, count) in histogram {
            if count > max_count {
                max_node = node_id;
                max_count = count;
            }
        }

        for instance in &instances {
            if instance.node_id == max_node {
                delete_database_instance(instance.id).await?;
                break;
            }
        }
    }
    Ok(())
}

async fn publish_schedule_state(node_id: u64) -> Result<(), anyhow::Error> {
    let sender = {
        let wci = WORKER_CONNECTION_INDEX.lock().unwrap();
        let connection = wci.get_client(&node_id).unwrap();
        connection.sender()
    };
    let database_instances = control_db::get_database_instances().await?;
    let databases = control_db::get_databases().await?;
    let schedule_state = ScheduleState {
        database_instances,
        databases,
    };
    let message = WorkerBoundMessage {
        r#type: Some(worker_bound_message::Type::ScheduleState(schedule_state)),
    };
    let mut buf = Vec::new();
    message.encode(&mut buf).unwrap();
    let result = sender.send(WebSocketMessage::Binary(buf)).await;
    if let Err(err) = result {
        log::debug!("{err}");
    }
    Ok(())
}

async fn broadcast_schedule_update(update: ScheduleUpdate) -> Result<(), anyhow::Error> {
    let mut senders = {
        let wci = WORKER_CONNECTION_INDEX.lock().unwrap();
        wci.connections.iter().map(|c| c.sender()).collect::<Vec<_>>()
    };

    for sender in senders.drain(..) {
        let message = WorkerBoundMessage {
            r#type: Some(worker_bound_message::Type::ScheduleUpdate(update.clone())),
        };
        let mut buf = Vec::new();
        message.encode(&mut buf).unwrap();
        let result = sender.send(WebSocketMessage::Binary(buf)).await;
        if let Err(err) = result {
            log::debug!("{err}");
        }
    }
    Ok(())
}

async fn send_budget_allocation(
    node_id: u64,
    identity: &Hash,
    sender: &WorkerConnectionSender,
    budget_allocation: &WorkerBudgetState,
) {
    let budget_update = BudgetUpdate {
        identity: identity.as_slice().to_vec(),
        allocation_delta: budget_allocation.delta_quanta,
    };
    let message = WorkerBoundMessage {
        r#type: Some(worker_bound_message::Type::BudgetUpdate(budget_update)),
    };
    let mut buf = Vec::new();
    message.encode(&mut buf).unwrap();

    let result = { sender.clone().send(WebSocketMessage::Binary(buf)).await };
    if let Err(err) = result {
        log::error!("Unable to send budget allocation to node {node_id} {err}");
    }
}

// Broadcast the budget allocations for only a single module to all worker nodes.
// Called when a specific module budget is updated.
pub(crate) async fn publish_energy_balance_state(identity: &Hash) -> Result<(), anyhow::Error> {
    // To avoid trying to hold the WCI mutex in the .awaits below, we'll pre-collect the node ids
    // here.
    let node_ids: Vec<_> = {
        let wci = WORKER_CONNECTION_INDEX.lock().unwrap();
        wci.connections.iter().map(|c| c.id).collect()
    };

    for node_id in node_ids {
        let allocation = budget_controller::identity_budget_allocations(node_id, identity).await;
        if let Some(allocation) = allocation {
            let sender = {
                let wci = WORKER_CONNECTION_INDEX.lock().unwrap();
                let connection = wci.get_client(&node_id).unwrap();
                connection.sender()
            };
            send_budget_allocation(node_id, identity, &sender, &allocation).await;
        }
    }

    Ok(())
}

/// Broadcast the current budget allocations to all worker nodes.
/// Called when a node is first connected and also on the budget refresh loop.
pub(crate) async fn node_publish_budget_state(node_id: u64) -> Result<(), anyhow::Error> {
    log::trace!("Sending budget state for node {}", node_id);
    let node_budget_allocations = budget_controller::budget_allocations(node_id).await;
    let node_budget_allocations = match node_budget_allocations {
        None => {
            log::warn!("Missing all budget allocations for node: {}", node_id);
            return Ok(());
        }
        Some(nba) => nba,
    };
    // For each budget allocation (per identity), copy it into a BudgetUpdate proto and
    // broadcast it to the node.
    // TODO: this is sending one message per identity. For efficiency we could consider a
    // single batched message containing all budgets.
    for nba in node_budget_allocations {
        let sender = {
            let wci = WORKER_CONNECTION_INDEX.lock().unwrap();
            let connection = wci.get_client(&node_id).unwrap();
            connection.sender()
        };
        send_budget_allocation(node_id, &nba.0, &sender, &nba.1).await;
    }
    Ok(())
}
