use clap::Command;
use spacetimedb_cli::Config;
use spacetimedb_paths::SpacetimePaths;

pub mod modules;
pub mod sdk;

#[track_caller]
pub fn invoke_cli(paths: &SpacetimePaths, args: &[&str]) {
    lazy_static::lazy_static! {
        static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        static ref COMMAND: Command = Command::new("spacetime").no_binary_name(true).subcommands(spacetimedb_cli::get_subcommands());
    }
    let config = Config::new_with_localhost(paths.cli_config_dir.cli_toml());

    let args = COMMAND.clone().get_matches_from(args);
    let (cmd, args) = args.subcommand().expect("Could not split subcommand and args");

    RUNTIME
        .block_on(spacetimedb_cli::exec_subcommand(config, paths, cmd, args))
        .unwrap();
}
