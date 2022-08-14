#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DatabaseInstanceState {
    #[prost(uint64, tag="1")]
    pub database_instance_id: u64,
    #[prost(bool, tag="2")]
    pub initialized: bool,
}
