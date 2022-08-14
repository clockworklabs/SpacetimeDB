// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct ClusterState {
//     #[prost(uint64, repeated, tag="1")]
//     pub nodes: ::std::vec::Vec<u64>,
//     #[prost(string, repeated, tag="2")]
//     pub advertise_addrs: ::std::vec::Vec<std::string::String>,
//     #[prost(uint64, tag="3")]
//     pub counter: u64,
// }
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct ConfChangeV2Context {
//     #[prost(string, repeated, tag="1")]
//     pub advertise_addrs: ::std::vec::Vec<std::string::String>,
// }
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct DataProposal {
//     #[prost(bytes, tag="1")]
//     pub data: std::vec::Vec<u8>,
// }
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct RaftMessage {
//     #[prost(bytes, tag="1")]
//     pub raft_message: std::vec::Vec<u8>,
// }
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct ConfChangeProposal {
//     #[prost(bytes, tag="1")]
//     pub conf_change_v2: std::vec::Vec<u8>,
// }
