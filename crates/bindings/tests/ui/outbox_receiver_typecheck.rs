#[path = "../fixtures/idc_receiver/lib.rs"]
mod receiver;
#[spacetimedb::table(accessor = outbound_ping, outbox(receiver::receive_ping))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: String,
    #[target]
    target: u8,
}

#[spacetimedb::table(accessor = wrong_body, outbox(receiver::receive_ping))]
pub struct WrongBody {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: u64,
    #[target]
    target: receiver::Identity,
}

#[spacetimedb::table(accessor = missing_body, outbox(receiver::receive_ping))]
pub struct MissingBody {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    payload: String,
}

#[spacetimedb::table(accessor = duplicate_body, outbox(receiver::receive_ping))]
pub struct DuplicateBody {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
    #[param(name = body)]
    payload: String,
}

#[spacetimedb::table(accessor = canonical_param_name, outbox(receiver::receive_ping))]
pub struct CanonicalParamName {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    #[param(name = Body)]
    payload: String,
}

fn main() {}
