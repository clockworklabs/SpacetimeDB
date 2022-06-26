/////// Generic Message //////
/// TODO: Theoretically this format could be replaced by TypeValue/TypeDef
/// but I don't think we want to do that yet.
/// TODO: Split this up into ServerBound and ClientBound if there's no overlap
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(oneof="message::Type", tags="1, 2, 3, 4")]
    pub r#type: ::std::option::Option<message::Type>,
}
pub mod message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Type {
        #[prost(message, tag="1")]
        FunctionCall(super::FunctionCall),
        #[prost(message, tag="2")]
        SubscriptionUpdate(super::SubscriptionUpdate),
        #[prost(message, tag="3")]
        Event(super::Event),
        #[prost(message, tag="4")]
        TransactionUpdate(super::TransactionUpdate),
    }
}
/// TODO: Evaluate if it makes sense for this to also include the
/// identity and name of the module this is calling
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FunctionCall {
    /// TODO: Maybe this should be replaced with an int identifier for performance?
    #[prost(string, tag="1")]
    pub reducer: std::string::String,
    #[prost(bytes, tag="2")]
    pub arg_bytes: std::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Event {
    #[prost(uint64, tag="1")]
    pub timestamp: u64,
    #[prost(bytes, tag="2")]
    pub caller_identity: std::vec::Vec<u8>,
    #[prost(message, optional, tag="3")]
    pub function_call: ::std::option::Option<FunctionCall>,
    /// TODO: arguably these should go inside an EventStatus message
    /// since success doesn't have a message
    #[prost(enumeration="event::Status", tag="4")]
    pub status: i32,
    #[prost(string, tag="5")]
    pub message: std::string::String,
}
pub mod event {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum Status {
        Committed = 0,
        Failed = 1,
    }
}
/// TODO: Maybe call this StateUpdate if it's implied to be a subscription update
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionUpdate {
    #[prost(message, repeated, tag="1")]
    pub table_updates: ::std::vec::Vec<TableUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TableUpdate {
    #[prost(uint32, tag="1")]
    pub table_id: u32,
    #[prost(message, repeated, tag="2")]
    pub table_row_operations: ::std::vec::Vec<TableRowOperation>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TableRowOperation {
    #[prost(enumeration="table_row_operation::OperationType", tag="1")]
    pub op: i32,
    #[prost(bytes, tag="2")]
    pub row_pk: std::vec::Vec<u8>,
    #[prost(bytes, tag="3")]
    pub row: std::vec::Vec<u8>,
}
pub mod table_row_operation {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum OperationType {
        Delete = 0,
        Insert = 1,
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionUpdate {
    #[prost(message, optional, tag="1")]
    pub event: ::std::option::Option<Event>,
    #[prost(message, optional, tag="2")]
    pub subscription_update: ::std::option::Option<SubscriptionUpdate>,
}
