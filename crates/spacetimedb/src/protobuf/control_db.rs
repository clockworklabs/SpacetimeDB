#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdentityEmail {
    #[prost(bytes, tag="1")]
    pub identity: std::vec::Vec<u8>,
    #[prost(string, tag="2")]
    pub email: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Database {
    #[prost(uint64, tag="1")]
    pub id: u64,
    #[prost(bytes, tag="2")]
    pub identity: std::vec::Vec<u8>,
    #[prost(string, tag="3")]
    pub name: std::string::String,
    #[prost(enumeration="HostType", tag="4")]
    pub host_type: i32,
    #[prost(uint32, tag="5")]
    pub num_replicas: u32,
    #[prost(bytes, tag="6")]
    pub program_bytes_address: std::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DatabaseStatus {
    #[prost(string, tag="2")]
    pub state: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DatabaseInstance {
    #[prost(uint64, tag="1")]
    pub id: u64,
    #[prost(uint64, tag="2")]
    pub database_id: u64,
    #[prost(uint64, tag="3")]
    pub node_id: u64,
    #[prost(bool, tag="4")]
    pub leader: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DatabaseInstanceStatus {
    #[prost(string, tag="2")]
    pub state: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Node {
    #[prost(uint64, tag="1")]
    pub id: u64,
    #[prost(bool, tag="2")]
    pub unschedulable: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NodeStatus {
    /// TODO: node memory, CPU, and storage capacity
    /// TODO: node memory, CPU, and storage allocatable capacity
    /// SEE: https://kubernetes.io/docs/reference/kubernetes-api/cluster-resources/node-v1/#NodeStatus
    #[prost(string, tag="1")]
    pub state: std::string::String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum HostType {
    Wasm32 = 0,
}
