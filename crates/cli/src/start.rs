use std::ffi::OsString;
use std::process::{Command, ExitCode};

use anyhow::Context;
use clap::{Arg, ArgMatches};
use spacetimedb_paths::SpacetimePaths;

pub fn cli() -> clap::Command {
    clap::Command::new("start")
        .about("Start a local SpacetimeDB instance")
        .disable_help_flag(true)
        .arg(
            Arg::new("edition")
                .long("edition")
                .help("The edition of SpacetimeDB to start up")
                .value_parser(clap::value_parser!(Edition))
                .default_value("standalone"),
        )
        .arg(
            Arg::new("args")
                .help("The args to pass to `spacetimedb-{edition} start`")
                .value_parser(clap::value_parser!(OsString))
                .allow_hyphen_values(true)
                .num_args(0..),
        )
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum Edition {
    Standalone,
    Cloud,
}

pub async fn exec(paths: &SpacetimePaths, args: &ArgMatches) -> anyhow::Result<ExitCode> {
    let edition = args.get_one::<Edition>("edition").unwrap();
    let args = args.get_many::<OsString>("args").unwrap_or_default();
    let bin_name = match edition {
        Edition::Standalone => "spacetimedb-standalone",
        Edition::Cloud => "spacetimedb-cloud",
    };
    let resolved_exe = std::env::current_exe().context("could not retrieve current exe")?;
    let bin_path = resolved_exe
        .parent()
        .unwrap()
        .join(bin_name)
        .with_extension(std::env::consts::EXE_EXTENSION);
    let mut cmd = Command::new(&bin_path);
    cmd.arg("start")
        .arg("--data-dir")
        .arg(&paths.data_dir)
        .arg("--jwt-key-dir")
        .arg(&paths.cli_config_dir)
        .args(args);

    // TODO(noa): use std::os::unix::process::CommandExt::exec() here once we have windows CI
    // use std::os::unix::process::CommandExt;
    // let err = cmd.exec();
    // Err(err).context(format!("exec failed for {}", bin_path.display()))

    let status = cmd
        .status()
        .with_context(|| format!("exec failed for {}", bin_path.display()))?;
    Ok(ExitCode::from(status.code().unwrap_or(1).try_into().unwrap_or(1)))
}
