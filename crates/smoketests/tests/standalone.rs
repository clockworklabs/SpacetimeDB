// Standalone-only test binary entry point.
//
// Tests in this target require control of a local SpacetimeDB server. They keep
// `require_local_server!()` as a defensive check even though CI selects this
// target only for standalone runs.
#[path = "standalone/mod.rs"]
mod smoketests;
