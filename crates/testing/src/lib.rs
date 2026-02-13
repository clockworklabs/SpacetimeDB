use clap::Command as ClapCommand;
use spacetimedb::messages::control_db::HostType;
use spacetimedb_cli::Config;
use spacetimedb_paths::SpacetimePaths;
use spacetimedb_schema::def::ModuleDef;
use std::env;
use std::process::Command;

pub mod modules;
pub mod sdk;

#[track_caller]
pub fn invoke_cli(paths: &SpacetimePaths, args: &[&str]) {
    lazy_static::lazy_static! {
        static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        static ref COMMAND: ClapCommand = ClapCommand::new("spacetime").no_binary_name(true).subcommands(spacetimedb_cli::get_subcommands());
    }

    // Parse once so we can decide which path to use.
    let matches = COMMAND.clone().get_matches_from(args);
    let (cmd, sub_args) = matches.subcommand().expect("Could not split subcommand and args");

    // If CUSTOM_SPACETIMEDB == true, try to use CUSTOM_SPACETIMEDB_PATH and shell out.
    let use_custom = env::var("CUSTOM_SPACETIMEDB")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if use_custom && cmd == "generate" {
        if let Ok(custom_path) = env::var("CUSTOM_SPACETIMEDB_PATH") {
            // Call the dev CLI exactly like the manual command:
            // cargo run --bin spacetimedb-cli -- generate ...
            let status = Command::new("cargo")
                .current_dir(&custom_path) // Ensure we run in the custom path directory
                .arg("run")
                .arg("--bin")
                .arg("spacetimedb-cli")
                .arg("--")
                .args(args) // `args` are like ["generate", "--lang", ...]
                .status()
                .expect("Failed to run custom SpacetimeDB CLI via cargo");

            assert!(status.success(), "Custom SpacetimeDB CLI failed");
            return;
        }
        // If CUSTOM_SPACETIMEDB_PATH is missing, fall through to the default behavior.
    }

    // Default: run in-process CLI (fast/path-friendly for tests).
    let config = Config::new_with_localhost(paths.cli_config_dir.cli_toml());
    RUNTIME
        .block_on(async {
            if cmd == "generate" {
                spacetimedb_cli::generate::exec_ex(config, sub_args, extract_descriptions).await
            } else {
                spacetimedb_cli::exec_subcommand(config, paths, None, cmd, sub_args)
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
            match wasm_file.extension().unwrap().to_str().unwrap() {
                "wasm" => HostType::Wasm,
                "js" => HostType::Js,
                _ => unreachable!(),
            },
        ))
    })
}
