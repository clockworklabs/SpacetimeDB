use spacetimedb_lib::Identity;
use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::ser::Serialize;

use super::control_db::*;

/// Messages from control node to worker node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkerBoundMessage {
    ScheduleState(ScheduleState),
    ScheduleUpdate(ScheduleUpdate),
    BudgetUpdate(BudgetUpdate),
}
/// Messages from worker node to control node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlBoundMessage {
    WorkerBudgetSpend(WorkerBudgetSpend),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleState {
    pub database_instances: Vec<DatabaseInstance>,
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
    DatabaseInstance(DatabaseInstance),
    Database(Database),
    Node(Node),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum UpdateOperation {
    DatabaseInstance(DatabaseInstance),
    Database(Database),
    Node(Node),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum DeleteOperation {
    DatabaseInstanceId(u64),
    DatabaseId(u64),
    NodeId(u64),
}
/// Budget allocation update from control node to worker node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetUpdate {
    pub identity: Identity,
    pub allocation_delta: i64,
}
/// Budget spend update from worker up to control node.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerBudgetSpend {
    pub identity_spend: Vec<WorkerModuleBudgetSpend>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerModuleBudgetSpend {
    pub identity: Identity,
    pub spend: i64,
}
