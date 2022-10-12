/// Messages from control node to worker node.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WorkerBoundMessage {
    #[prost(oneof="worker_bound_message::Type", tags="1, 2, 3")]
    pub r#type: ::std::option::Option<worker_bound_message::Type>,
}
pub mod worker_bound_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(message, tag="1")]
        ScheduleState(super::ScheduleState),
        #[prost(message, tag="2")]
        ScheduleUpdate(super::ScheduleUpdate),
        #[prost(message, tag="3")]
        BudgetUpdate(super::BudgetUpdate),
    }
}
/// Messages from worker node to control node.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ControlBoundMessage {
    #[prost(oneof="control_bound_message::Type", tags="1")]
    pub r#type: ::std::option::Option<control_bound_message::Type>,
}
pub mod control_bound_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(message, tag="1")]
        WorkerBudgetSpend(super::WorkerBudgetSpend),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScheduleState {
    #[prost(message, repeated, tag="1")]
    pub database_instances: ::std::vec::Vec<super::control_db::DatabaseInstance>,
    #[prost(message, repeated, tag="2")]
    pub databases: ::std::vec::Vec<super::control_db::Database>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScheduleUpdate {
    #[prost(oneof="schedule_update::Type", tags="1, 2, 3")]
    pub r#type: ::std::option::Option<schedule_update::Type>,
}
pub mod schedule_update {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(message, tag="1")]
        Insert(super::InsertOperation),
        #[prost(message, tag="2")]
        Update(super::UpdateOperation),
        #[prost(message, tag="3")]
        Delete(super::DeleteOperation),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InsertOperation {
    #[prost(oneof="insert_operation::Type", tags="1, 2")]
    pub r#type: ::std::option::Option<insert_operation::Type>,
}
pub mod insert_operation {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(message, tag="1")]
        DatabaseInstance(super::super::control_db::DatabaseInstance),
        #[prost(message, tag="2")]
        Database(super::super::control_db::Database),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpdateOperation {
    #[prost(oneof="update_operation::Type", tags="1, 2")]
    pub r#type: ::std::option::Option<update_operation::Type>,
}
pub mod update_operation {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(message, tag="1")]
        DatabaseInstance(super::super::control_db::DatabaseInstance),
        #[prost(message, tag="2")]
        Database(super::super::control_db::Database),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteOperation {
    #[prost(oneof="delete_operation::Type", tags="1, 2")]
    pub r#type: ::std::option::Option<delete_operation::Type>,
}
pub mod delete_operation {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(uint64, tag="1")]
        DatabaseInstanceId(u64),
        #[prost(uint64, tag="2")]
        DatabaseId(u64),
    }
}
/// Budget allocation update from control node to worker node.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BudgetUpdate {
    #[prost(bytes, tag="1")]
    pub identity: std::vec::Vec<u8>,
    #[prost(int64, tag="2")]
    pub allocation_delta: i64,
}
/// Budget spend update from worker up to control node.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WorkerBudgetSpend {
    #[prost(message, repeated, tag="1")]
    pub identity_spend: ::std::vec::Vec<WorkerModuleBudgetSpend>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WorkerModuleBudgetSpend {
    #[prost(bytes, tag="1")]
    pub identity: std::vec::Vec<u8>,
    #[prost(int64, tag="2")]
    pub spend: i64,
}
