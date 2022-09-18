//! Support for the control-node side of budget management.
//!
use std::{collections::HashMap, sync::Mutex};

use anyhow::anyhow;

use crate::hash::Hash;
use crate::nodes::control_node::control_db;
use crate::nodes::control_node::control_db::set_energy_budget;
use crate::protobuf::control_db::EnergyBudget;

// TODO: Consider making not static & dependency injected
lazy_static::lazy_static! {
    static ref GLOBAL_MODULE_BUDGET : Mutex<HashMap<Hash, EnergyBudget>> = Mutex::new(HashMap::new());
    static ref NODE_MODULE_BUDGET : Mutex<HashMap<(u64 /* node_id */, Hash), WorkerBudgetState>> = Mutex::new(HashMap::new());
}

/// Represents the current state of a given worker node's budget.
#[derive(Copy, Clone)]
pub(crate) struct WorkerBudgetState {
    // The total allocated to this identity on this node. Declines as they spend.
    pub allocation_quanta: i64,
    // How much the worker used in the last interval for this identity
    pub interval_used_quanta: i64,
    // How much the worker has used total, since it came up (for this identity).
    pub total_used_quanta: i64,
    // The delta that will be sent to the node in the next interval. This starts as =
    // allocation_quanta, and then is calculated as "last allocation_quanta - newly calculated
    // quanta" on each refresh cycle.
    pub delta_quanta: i64,
    // How much the default maximum spend is, per-reducer.
    pub default_max_spend_quanta: i64,
}

/// Set the global budget for a given identity.
pub(crate) async fn set_module_budget(module_identity: &Hash, budget: &EnergyBudget) {
    // Fill the write-through global budget cache first.
    {
        let mut identity_budget = GLOBAL_MODULE_BUDGET.lock().expect("unlock ctrl budget");
        identity_budget.insert(module_identity.clone(), budget.clone());
    }

    // Now persist it.
    // TODO(ryan): Is failure here a legit case for panic? It seems it, but we might want to revisit later.
    control_db::set_energy_budget(module_identity, budget)
        .expect("Unable to write-through updated budget to control_db");

    // Refresh this identity's budget allocations for all nodes based on the new balance information
    update_budget_allocation(module_identity, &budget).await;
}

/// Retrieve the global budget for a given identity.
pub(crate) fn get_module_budget(module_identity: &Hash) -> Option<EnergyBudget> {
    let identity_budget = GLOBAL_MODULE_BUDGET.lock().expect("unlock ctrl budget");
    identity_budget.get(module_identity).map(|b| b.clone())
}

/// Refresh the budget state for all known identities and nodes.
/// Called on startup to establish initial budget state.
// TODO(ryan): Many assumptions here that break down if there's a control node being added to an
// existing cluster, etc. In reality we'll need to poll nodes for their initial current 'used'
// state and readjust allocations accordingly.
pub(crate) async fn refresh_all_budget_allocations() {
    // Fill identity -> global budget cache
    let budgets = {
        let budgets = control_db::get_energy_budgets().await.expect("retrieve all budgets");
        budgets
    };
    for eb in budgets.iter() {
        let identity = Hash::from_slice(eb.module_identity.as_slice());

        // Populate top-level master budget
        {
            let mut identity_budget = GLOBAL_MODULE_BUDGET.lock().expect("unlock ctrl budget");
            identity_budget.insert(identity, eb.clone());
        }

        // Update the per-worker state
        update_identity_worker_budget_state(&identity, &eb).await;
    }
}

// Refresh budget allocation for a single identity.
pub(crate) async fn update_budget_allocation(module_identity: &Hash, eb: &EnergyBudget) {
    // Fill identity -> global budget cache
    let budget = control_db::get_energy_budget(module_identity)
        .await
        .expect("retrieve identity budget");
    if budget.is_none() {
        log::warn!("No budget for identity: {}", module_identity.to_hex());
        return;
    }

    // Populate top-level master budget for the identity.
    {
        let mut identity_budget = GLOBAL_MODULE_BUDGET.lock().expect("unlock ctrl budget");
        identity_budget.insert(*module_identity, eb.clone());
    }
    update_identity_worker_budget_state(&module_identity, eb).await;
}

/// Initial state. Delta quanta is the whole amount.
fn initial_budget_state(per_node_quanta: i64, default_max_spend_quanta: u64) -> WorkerBudgetState {
    WorkerBudgetState {
        allocation_quanta: per_node_quanta,
        interval_used_quanta: 0,
        total_used_quanta: 0,
        delta_quanta: per_node_quanta,
        default_max_spend_quanta: default_max_spend_quanta as i64,
    }
}

/// Calculate what the portion of budget for a given worker node should be.
// TODO: right now this is just a brute simple "budget divided by number of nodes" but there is
// room for more sophistication here in the future.
fn calculate_per_node_quanta(eb: &EnergyBudget, _worker_node_id: u64, number_of_nodes: usize) -> i64 {
    eb.balance_quanta / (number_of_nodes as i64)
}

/// Set per-node budget partitions. Called by both initial setup and on the budget refresh loop.
async fn update_identity_worker_budget_state(module_identity: &Hash, eb: &EnergyBudget) {
    let nodes = { control_db::get_nodes().await.expect("retrieve all nodes") };
    let num_nodes = nodes.len();
    for node in nodes {
        let per_node_quanta = calculate_per_node_quanta(&eb, node.id, num_nodes);
        let mut node_identity_budget = NODE_MODULE_BUDGET.lock().expect("unlock node/identity budget state");
        let budget_entry = node_identity_budget.entry((node.id, *module_identity));
        budget_entry
            .and_modify(|bs| {
                let node_new_allocation = per_node_quanta;
                let new_delta = node_new_allocation - bs.allocation_quanta;
                log::debug!(
                    "Delta for node {} is {} (interval spend {} / {} total, previously allocated {}, and new allocation {})",
                    node.id,
                    new_delta,
                    bs.interval_used_quanta,
                    bs.total_used_quanta,
                    bs.allocation_quanta,
                    node_new_allocation
                );

                // Update relevant new state that will be sent to the client for refresh.
                bs.delta_quanta = new_delta;
                bs.allocation_quanta = node_new_allocation;
                bs.interval_used_quanta = 0;
            })
            .or_insert(initial_budget_state(per_node_quanta, eb.default_reducer_maximum_quanta));
    }
}

/// Retrieve current budget allocations for a given node for the current interval.
/// After this is sent, the allocation delta for the next interval is re-adjusted for the next
/// interval.
pub(crate) async fn budget_allocations(node_id: u64) -> Option<Vec<(Hash, WorkerBudgetState)>> {
    let node_identity_budget = NODE_MODULE_BUDGET.lock().expect("unlock node/identity budget state");
    let node_entries = node_identity_budget.iter().filter(|entry| entry.0 .0 == node_id);
    let x = node_entries.map(|entry| (entry.0 .1, entry.1.clone()));
    Some(x.collect())
}

/// Called by the worker_connection when budget spend information is received from a node.
// TODO: what happens when we lose contact with a worker? Budget re-allocation necessary?
pub(crate) fn node_budget_spend_update(node_id: u64, module_identity: &Hash, spend: i64) -> Result<(), anyhow::Error> {
    log::debug!(
        "Worker {} identity: {} spent: {}",
        node_id,
        module_identity.to_hex(),
        spend
    );

    let mut node_budgets = NODE_MODULE_BUDGET.lock().expect("unlock node identity budget");
    let node_budget = node_budgets.get_mut(&(node_id, *module_identity));
    match node_budget {
        None => Err(anyhow!(
            "Missing budget record for identity {} in worker node {}",
            module_identity.to_hex(),
            node_id
        )),
        Some(mut budget) => {
            // First update the total balance by subtracting the known spend from the total budget.
            let mut identity_budget = GLOBAL_MODULE_BUDGET.lock().expect("unlock identity budget");
            match identity_budget.get_mut(module_identity) {
                None => {
                    return Err(anyhow!(
                        "Unable to find global energy budget for identity: {}",
                        module_identity.to_hex()
                    ))
                }
                Some(total_budget) => {
                    total_budget.balance_quanta -= spend;
                    set_energy_budget(module_identity, total_budget).unwrap();
                }
            };

            // And then record that the worker consumed this much energy...
            budget.interval_used_quanta += spend;
            budget.total_used_quanta += spend;

            Ok(())
        }
    }
}
