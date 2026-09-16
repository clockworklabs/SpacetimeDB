use spacetimedb::MigrationContext;
use spacetimedb::Table;

#[spacetimedb::migration(
    function = migration,
    hash = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff,
)]
mod dropped {
    #[spacetimedb::table(accessor = abc)]
    struct Abc {
        pub x: u8,
        pub y: u32,
    }
}

fn migration(ctx: &MigrationContext<Dropped>) {
    for abc in ctx.dropped.abc().iter() {
        abc.x;
    }
    // ctx.dropped
}
