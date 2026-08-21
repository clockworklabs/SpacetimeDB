#[spacetimedb::table(accessor = option_index_args)]
struct OptionIndexArgs {
    #[primary_key]
    id: u64,
    #[index(btree)]
    option_u64: Option<u64>,
}

#[spacetimedb::table(accessor = compound_option_index_args, index(accessor = by_id_and_option, btree(columns = [id, option_u64])))]
struct CompoundOptionIndexArgs {
    id: u64,
    option_u64: Option<u64>,
}

#[spacetimedb::reducer]
fn option_index_filters_compile(ctx: &spacetimedb::ReducerContext) {
    let some_u64 = Some(55u64);
    let none_u64: Option<u64> = None;

    let _ = ctx.db.option_index_args().option_u64().filter(some_u64);
    let _ = ctx.db.option_index_args().option_u64().filter(none_u64);
    let _ = ctx.db.option_index_args().option_u64().filter(Some(1u64)..Some(99u64));
    let _ = ctx.db.option_index_args().option_u64().filter(None..=Some(99u64));

    let _ = ctx
        .db
        .compound_option_index_args()
        .by_id_and_option()
        .filter((1u64, Some(55u64)));
    let _ = ctx
        .db
        .compound_option_index_args()
        .by_id_and_option()
        .filter((1u64, None..=Some(99u64)));
}

fn main() {}
