pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn long_version() -> String {
    format!(
        "\
Path: {path}
Commit: {commit}
spacetimedb tool version {CLI_VERSION}; spacetimedb-lib version {lib_ver};",
        path = std::env::current_exe().unwrap_or_else(|_| "<unknown>".into()).display(),
        commit = env!("GIT_HASH"),
        lib_ver = spacetimedb_lib::version::spacetimedb_lib_version()
    )
}
