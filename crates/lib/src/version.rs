const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HASH: &str = env!("GIT_HASH");

pub fn spacetimedb_lib_version() -> &'static str {
    CLI_VERSION
}
