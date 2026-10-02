//! Stand-ins for the SDK traits, in their own crate so they are foreign to `projections`.
pub trait InModule {
    type Module: SpacetimeModule;
}
pub trait SpacetimeModule {
    type DbView;
    type Reducer;
    type ReducerEventContext;
}
pub trait ClientTableDef {
    type Row;
    const NAME: &'static str;
}
