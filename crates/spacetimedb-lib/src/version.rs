const CLI_VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn spacetimedb_lib_version() -> &'static str {
    CLI_VERSION
}
