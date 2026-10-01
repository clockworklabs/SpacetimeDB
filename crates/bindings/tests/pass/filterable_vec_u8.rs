use spacetimedb::{DbContext, ReducerContext};

#[spacetimedb::table(
    accessor = byte_rows,
    index(accessor = by_bytes, btree(columns = [bytes]))
)]
struct ByteRow {
    bytes: Vec<u8>,
}

#[spacetimedb::reducer]
fn filter_byte_rows(ctx: &ReducerContext) {
    let owned = vec![0, 1, 2, 0xff];

    let _ = ctx.db.byte_rows().by_bytes().filter(&owned);
    let _ = ctx.db.byte_rows().by_bytes().filter(owned.as_slice());
    let _ = ctx.db.byte_rows().by_bytes().filter(&[][..]);
}

fn main() {}
