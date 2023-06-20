use spacetimedb::stdb_path;
use std::env;
use std::path::Path;

pub mod modules;

pub fn set_key_env_vars() {
    let set_if_not_exist = |var, path| {
        if env::var_os(var).is_none() {
            env::set_var(var, Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path));
        }
    };
    set_if_not_exist("SPACETIMEDB_JWT_PUB_KEY", stdb_path("id_ecdsa.pub"));
    set_if_not_exist("SPACETIMEDB_JWT_PRIV_KEY", stdb_path("id_ecdsa"));
}
