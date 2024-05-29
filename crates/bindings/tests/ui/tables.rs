use spacetimedb::*;

struct Test;

#[spacetimedb(table)]
struct Table {
    x: Test,
}

#[spacetimedb(table)]
struct TypeParam<T> {
    t: T,
}

#[spacetimedb(table)]
struct ConstParam<const X: u8> {}

fn main() {}
