#[spacetimedb::table(accessor = messages, outbox(send_message))]
struct Message {
    #[primary_key]
    #[auto_inc]
    row_id: u64,
    body: String,
}

fn main() {}
