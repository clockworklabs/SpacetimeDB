use std::env;
use std::path::{Path, PathBuf};

pub mod modules;

pub fn set_key_env_vars() {
    let set_if_not_exist = |var, path| {
        if env::var_os(var).is_none() {
            env::set_var(var, Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path));
        }
    };

    set_if_not_exist("STDB_PATH", PathBuf::from("/stdb"));
    set_if_not_exist("SPACETIMEDB_LOGS_PATH", PathBuf::from("/var/log"));
    set_if_not_exist("SPACETIMEDB_LOG_CONFIG", PathBuf::from("/etc/spacetimedb/log.conf"));
    set_if_not_exist(
        "SPACETIMEDB_JWT_PUB_KEY",
        PathBuf::from("/etc/spacetimedb/id_ecdsa.pub"),
    );
    set_if_not_exist("SPACETIMEDB_JWT_PRIV_KEY", PathBuf::from("/etc/spacetimedb/id_ecdsa"));
}
