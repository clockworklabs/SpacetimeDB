use spacetimedb_lib::Identity;
use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::ser::Serialize;

use super::control_db::*;

/// Messages from control node to worker node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkerBoundMessage {
    ScheduleState(ScheduleState),
    ScheduleUpdate(ScheduleUpdate),
    EnergyBalanceState(EnergyBalanceState),
    EnergyBalanceUpdate(EnergyBalanceUpdate),
}
/// Messages from worker node to control node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlBoundMessage {
    EnergyWithdrawals(EnergyWithdrawals),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleState {
    pub replicas: Vec<Replica>,
    pub databases: Vec<Database>,
    pub nodes: Vec<Node>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum ScheduleUpdate {
    Insert(InsertOperation),
    Update(UpdateOperation),
    Delete(DeleteOperation),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum InsertOperation {
    Replica(Replica),
    Database(Database),
    Node(Node),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum UpdateOperation {
    Replica(Replica),
    Database(Database),
    Node(Node),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum DeleteOperation {
    ReplicaId(u64),
    DatabaseId(u64),
    NodeId(u64),
}
/// An energy balance update from control node to worker node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyBalanceUpdate {
    pub identity: Identity,
    pub energy_balance: i128,
}
// A message to syncronize energy balances from control node to worker node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyBalanceState {
    pub balances: Vec<EnergyBalance>,
}
/// Budget spend update from worker up to control node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyWithdrawals {
    pub withdrawals: Vec<EnergyWithdrawal>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyWithdrawal {
    pub identity: Identity,
    pub amount: i128,
}
