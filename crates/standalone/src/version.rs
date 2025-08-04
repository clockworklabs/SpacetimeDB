pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn long_version() -> String {
    format!(
        "spacetimedb tool version {CLI_VERSION}; spacetimedb-lib version {};",
        spacetimedb_lib::version::spacetimedb_lib_version()
    )
}
