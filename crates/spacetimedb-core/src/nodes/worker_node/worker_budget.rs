use crate::hash::Hash;

use crate::protobuf::control_worker_api::{
    control_bound_message, ControlBoundMessage, WorkerBudgetSpend, WorkerModuleBudgetSpend,
};
use futures::SinkExt;
use prost::Message;
use std::cmp::min;
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
    /// The maximum we can spend per reducer call.
    default_max_spend_quanta: i64,
}

struct WorkerBudgets {
    // Mapping of identity to current per-worker budget information.
    // Allocation is distributed (set) by control node.
    // Used by hosts.
    // Then control node is told about spend, and budget re-allocated accordingly.
    module_budget: HashMap<Hash, WorkerBudget>,
}

impl WorkerBudgets {
    pub fn new() -> Self {
        Self {
            module_budget: HashMap::new(),
        }
    }
}

/// Get the maximum amount we should spend on the next transaction.
/// (Returns the default maximum spend or the remaining balance, whichever is least.)
pub(crate) fn max_tx_spend(module_identity: &Hash) -> i64 {
    let budgets = BUDGETS.lock().expect("budgets lock");
    budgets
        .module_budget
        .get(&module_identity)
        .map(|b| min(b.allocation_quanta - b.used_quanta, b.default_max_spend_quanta))
        .unwrap_or_else(|| 0)
}

/// Called by host controller to register spending for a given identity.
/// Returns remaining balance (from this node's current allocation)
pub(crate) fn record_tx_spend(module_identity: &Hash, spent_quanta: i64) -> i64 {
    let mut budgets = BUDGETS.lock().expect("budgets lock");
    log::trace!(
        "Subtracting {} from ledger for {}",
        spent_quanta,
        module_identity.to_hex()
    );
    budgets
        .module_budget
        .get_mut(module_identity)
        .map(|b| {
            b.used_quanta += spent_quanta;
            b.allocation_quanta - b.used_quanta
        })
        .unwrap_or_else(|| {
            // We don't have any budgeting information for this identity yet. So put in a (hopefully
            // temporary) negative balance.
            budgets.module_budget.insert(
                *module_identity,
                WorkerBudget {
                    allocation_quanta: spent_quanta,
                    used_quanta: 0,
                    default_max_spend_quanta: 0,
                },
            );
            -spent_quanta
        })
}

/// Called by control node to add to (or remove from) a node's current budget allocation and
/// default spend.
pub(crate) fn on_budget_receive_allocation(module_identity: &Hash, allocation_delta: i64, default_max_spend: i64) {
    log::debug!(
        "Received budget allocation with delta: {} & default_max_spend: {}",
        allocation_delta,
        default_max_spend
    );
    let mut budgets = BUDGETS.lock().expect("budgets lock");
    budgets
        .module_budget
        .get_mut(module_identity)
        .map(|b| {
            b.allocation_quanta += allocation_delta;
            b.default_max_spend_quanta = default_max_spend;

            // Reset the used quanta, because we got a new update.
            b.used_quanta = 0;
        })
        .unwrap_or_else(|| {
            // Receiving the initial budget allocation.
            budgets.module_budget.insert(
                *module_identity,
                WorkerBudget {
                    allocation_quanta: allocation_delta,
                    used_quanta: 0,
                    default_max_spend_quanta: default_max_spend,
                },
            );
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
            module_identity_spend: budgets
                .module_budget
                .iter()
                .map(|b| WorkerModuleBudgetSpend {
                    module_identity: b.0.as_slice().to_vec(),
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
