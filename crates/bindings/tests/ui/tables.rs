struct Test;

#[spacetimedb::table(name = table)]
struct Table {
    x: Test,
}

#[spacetimedb::table(name = type_param)]
struct TypeParam<T> {
    t: T,
}

#[spacetimedb::table(name = const_param)]
struct ConstParam<const X: u8> {}

fn main() {}
