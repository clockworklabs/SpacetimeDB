use spacetimedb::{
    sats::{i256, u256},
    table, ConnectionId, TimeDuration, Uuid,
};

#[table(accessor = special_value, public)]
pub struct SpecialValue {
    #[primary_key]
    pub id: u64,
    pub uuid: Uuid,
    pub connection_id: ConnectionId,
    pub duration: TimeDuration,
    pub unsigned_128: u128,
    pub signed_128: i128,
    pub unsigned_256: u256,
    pub signed_256: i256,
}
