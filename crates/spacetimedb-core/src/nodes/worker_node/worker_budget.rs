use crate::hash::Hash;

use crate::nodes::worker_node::worker_metrics::NODE_IDENTITY_ENERGY_BUDGET_GAUGE;
use crate::protobuf::control_worker_api::{
    control_bound_message, ControlBoundMessage, WorkerBudgetSpend, WorkerModuleBudgetSpend,
};
use futures::SinkExt;
use prost::Message;
use std::{collections::HashMap, sync::Mutex};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

// TODO: Consider moving directly onto the host_controller, rather than holding here
// TODO: content of this struct could probably be atomic ints and avoid locking.  maybe.
lazy_static::lazy_static! {
    static ref BUDGETS: Mutex<WorkerBudgets> = Mutex::new(WorkerBudgets::new());
}

struct WorkerBudget {
    /// How much we've been allocated (total)
    allocation_quanta: i64,
    /// How much we've used since our last allocation update. (Reset at each allocation update)
    used_quanta: i64,
}

struct WorkerBudgets {
    // Mapping of identity to current per-worker budget information.
    // Allocation is distributed (set) by control node.
    // Used by hosts.
    // Then control node is told about spend, and budget re-allocated accordingly.
    identity_budget: HashMap<Hash, WorkerBudget>,
}

impl WorkerBudgets {
    pub fn new() -> Self {
        Self {
            identity_budget: HashMap::new(),
        }
    }
}

/// Get the maximum amount we should spend on the next transaction.
/// (Returns the default maximum spend or the remaining balance, whichever is least.)
pub(crate) fn max_tx_spend(identity: &Hash) -> i64 {
    let budgets = BUDGETS.lock().expect("budgets lock");
    budgets
        .identity_budget
        .get(&identity)
        .map(|b| b.allocation_quanta - b.used_quanta)
        .unwrap_or_else(|| 0)
}

/// Called by host controller to register spending for a given identity.
/// Returns remaining balance (from this node's current allocation)
pub(crate) fn record_tx_spend(identity: &Hash, spent_quanta: i64) -> i64 {
    let mut budgets = BUDGETS.lock().expect("budgets lock");
    log::trace!("Subtracting {} from ledger for {}", spent_quanta, identity.to_hex());
    budgets
        .identity_budget
        .get_mut(identity)
        .map(|b| {
            b.used_quanta += spent_quanta;
            b.allocation_quanta - b.used_quanta
        })
        .unwrap_or_else(|| {
            // We don't have any budgeting information for this identity yet. So put in a (hopefully
            // temporary) negative balance.
            budgets.identity_budget.insert(
                *identity,
                WorkerBudget {
                    allocation_quanta: spent_quanta,
                    used_quanta: 0,
                },
            );
            -spent_quanta
        })
}

/// Called by control node to add to (or remove from) a node's current budget allocation and
/// default spend.
pub(crate) fn on_budget_receive_allocation(node_id: u64, identity: &Hash, allocation_delta: i64) {
    log::debug!("Received budget allocation with delta: {}", allocation_delta,);
    let mut budgets = BUDGETS.lock().expect("budgets lock");
    budgets
        .identity_budget
        .get_mut(identity)
        .map(|b| {
            b.allocation_quanta += allocation_delta;

            // Reset the used quanta, because we got a new update.
            b.used_quanta = 0;

            NODE_IDENTITY_ENERGY_BUDGET_GAUGE
                .with_label_values(&[identity.to_hex().as_str(), format!("{}", node_id).as_str()])
                .set(b.allocation_quanta as f64);
        })
        .unwrap_or_else(|| {
            // Receiving the initial budget allocation.
            budgets.identity_budget.insert(
                *identity,
                WorkerBudget {
                    allocation_quanta: allocation_delta,
                    used_quanta: 0,
                },
            );

            NODE_IDENTITY_ENERGY_BUDGET_GAUGE
                .with_label_values(&[identity.to_hex().as_str(), format!("{}", node_id).as_str()])
                .set(allocation_delta as f64);
        })
}

/// Sends a message to the control node with the node's spend since the last update.
/// Called by the client connection spend update loop in order to produce spending updates for
/// the control node.
/// The control node uses this information to calculate the next delta.
pub(crate) async fn send_budget_alloc_spend(
    socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> Result<(), Error> {
    let spend_msg = {
        let budgets = BUDGETS.lock().expect("budgets lock");
        WorkerBudgetSpend {
            identity_spend: budgets
                .identity_budget
                .iter()
                .map(|b| WorkerModuleBudgetSpend {
                    identity: b.0.as_slice().to_vec(),
                    spend: b.1.used_quanta,
                })
                .collect(),
        }
    };
    let message = ControlBoundMessage {
        r#type: Some(control_bound_message::Type::WorkerBudgetSpend(spend_msg)),
    };
    let mut buf = Vec::new();
    message.encode(&mut buf).unwrap();

    // How do we send? We don't have access to the socket.
    socket.send(WebSocketMessage::Binary(buf)).await
}
