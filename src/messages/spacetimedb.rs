#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TableWrite {
    #[prost(uint32, tag="1")]
    pub table_id: u32,
    #[prost(message, repeated, tag="2")]
    pub writes: ::std::vec::Vec<Write>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Insert {
    #[prost(oneof="insert::Insert", tags="1, 2")]
    pub insert: ::std::option::Option<insert::Insert>,
}
pub mod insert {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Insert {
        #[prost(bytes, tag="1")]
        Hash(std::vec::Vec<u8>),
        #[prost(bytes, tag="2")]
        Raw(std::vec::Vec<u8>),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Write {
    #[prost(oneof="write::Operation", tags="1, 2")]
    pub operation: ::std::option::Option<write::Operation>,
}
pub mod write {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Operation {
        #[prost(message, tag="1")]
        Insert(super::Insert),
        #[prost(bytes, tag="2")]
        Delete(std::vec::Vec<u8>),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Commit {
    #[prost(bytes, tag="1")]
    pub parent_commit_hash: std::vec::Vec<u8>,
    #[prost(message, repeated, tag="2")]
    pub table_writes: ::std::vec::Vec<TableWrite>,
}
