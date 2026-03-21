// Enums with explicit discriminant values should produce a compile error,
// since SATS assigns variant tags by declaration order and ignores discriminants.

#[derive(spacetimedb::SpacetimeType)]
enum ExplicitValues {
    A = 1,
    B = 2,
}

#[derive(spacetimedb::SpacetimeType)]
enum MixedValues {
    X,
    Y = 5,
}

// This should compile fine — no explicit discriminants.
#[derive(spacetimedb::SpacetimeType)]
enum NoValues {
    Foo,
    Bar,
}

fn main() {}
