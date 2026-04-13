struct Test;

#[spacetimedb::table(accessor = table)]
struct Table {
    x: Test,
}

#[spacetimedb::table(accessor = type_param)]
struct TypeParam<T> {
    t: T,
}

#[spacetimedb::table(accessor = const_param)]
struct ConstParam<const X: u8> {}

#[derive(spacetimedb::SpacetimeType)]
struct Alpha {
    beta: u8,
}

#[spacetimedb::table(accessor = delta)]
struct Delta {
    #[unique]
    #[index(btree)]
    compound_a: Alpha,
    #[index(btree)]
    compound_b: Alpha,
}

#[spacetimedb::reducer]
fn bad_filter_on_index(ctx: &spacetimedb::ReducerContext) {
    ctx.db.delta().compound_a().find(Alpha { beta: 0 });
    ctx.db.delta().compound_b().filter(Alpha { beta: 1 });
}

fn main() {}
