// Standalone test binary entry point.
//
// These smoketests are assigned to standalone coverage. Some require control
// of a local SpacetimeDB server; others simply provide no additional value when
// repeated against a cluster. Tests that require local server control keep
// `require_local_server!()` as a defensive check.
#[path = "standalone/mod.rs"]
mod smoketests;
