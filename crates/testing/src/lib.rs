use clap::Command;
use spacetimedb::config::{FilesLocal, SpacetimeDbFiles};
use spacetimedb_cli::Config;
use std::env;

pub mod modules;
pub mod sdk;

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

pub fn invoke_cli(args: &[&str]) {
    lazy_static::lazy_static! {
        static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        static ref COMMAND: Command = Command::new("spacetime").no_binary_name(true).subcommands(spacetimedb_cli::get_subcommands());
        static ref CONFIG: Config = Config::new_with_localhost();
    }

    let args = COMMAND.clone().get_matches_from(args);
    let (cmd, args) = args.subcommand().expect("Could not split subcommand and args");

    RUNTIME
        .block_on(spacetimedb_cli::exec_subcommand((*CONFIG).clone(), cmd, args))
        .unwrap()
}
