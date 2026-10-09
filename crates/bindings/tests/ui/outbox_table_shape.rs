#[path = "../fixtures/idc_receiver/lib.rs"]
mod receiver;
#[spacetimedb::table(accessor = missing_pk, outbox(receiver::receive_ping))]
pub struct MissingPk {
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = non_auto_inc_pk, outbox(receiver::receive_ping))]
pub struct NonAutoIncPk {
    #[primary_key]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = wrong_pk_type, outbox(receiver::receive_ping))]
pub struct WrongPkType {
    #[primary_key]
    #[auto_inc]
    id: u32,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = scheduled_and_outbox, scheduled(scheduled), outbox(receiver::receive_ping))]
pub struct ScheduledAndOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = callback_outbox, outbox(receiver::receive_ping))]
pub struct CallbackOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::reducer(on_result(callback_outbox))]
pub fn bad_on_result(_ctx: &spacetimedb::ReducerContext, _row: CallbackOutbox) {}

fn main() {}
