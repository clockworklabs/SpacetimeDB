pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CLI_GIT_HASH: &str = env!("GIT_HASH");

pub fn long_version() -> String {
    format!(
        "\
Path: {path}
Commit: {commit}
spacetimedb tool version {CLI_VERSION}; spacetimedb-lib version {lib_ver};",
        path = std::env::current_exe().unwrap_or_else(|_| "<unknown>".into()).display(),
        commit = CLI_GIT_HASH,
        lib_ver = spacetimedb_lib::version::spacetimedb_lib_version()
    )
}
