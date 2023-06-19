use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::ser::Serialize;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInstanceState {
    pub database_instance_id: u64,
    pub initialized: bool,
}
