use clap::Command;
use spacetimedb_cli::Config;
use spacetimedb_paths::SpacetimePaths;
use spacetimedb_schema::def::ModuleDef;

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
        .block_on(async {
            if cmd == "generate" {
                spacetimedb_cli::generate::exec_ex(config, args, extract_descriptions).await
            } else {
                spacetimedb_cli::exec_subcommand(config, paths, None, cmd, args)
                    .await
                    .map(drop)
            }
        })
        .unwrap();
}

// spacetime generate would usually shell out to spacetimedb-standalone,
// but that won't work in a testing environment
fn extract_descriptions(wasm_file: &std::path::Path) -> anyhow::Result<ModuleDef> {
    tokio::task::block_in_place(|| {
        let program_bytes = std::fs::read(wasm_file)?;
        tokio::runtime::Handle::current().block_on(spacetimedb::host::extract_schema(
            program_bytes.into(),
            spacetimedb::messages::control_db::HostType::Wasm,
        ))
    })
}
