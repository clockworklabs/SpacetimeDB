use spacetimedb::config::{FilesLocal, SpacetimeDbFiles};
use std::env;

pub mod modules;

pub fn set_key_env_vars(paths: &FilesLocal) {
    let set_if_not_exist = |var, path| {
        if env::var_os(var).is_none() {
            env::set_var(var, path);
        }
    };

    set_if_not_exist("STDB_PATH", paths.db_path());
    set_if_not_exist("SPACETIMEDB_LOGS_PATH", paths.logs());
    set_if_not_exist("SPACETIMEDB_LOG_CONFIG", paths.log_config());
    set_if_not_exist("SPACETIMEDB_JWT_PUB_KEY", paths.public_key());
    set_if_not_exist("SPACETIMEDB_JWT_PRIV_KEY", paths.private_key());
}
