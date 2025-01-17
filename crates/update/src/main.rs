use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use spacetimedb_paths::{RootDir, SpacetimePaths};

mod proxy;

fn main() -> anyhow::Result<ExitCode> {
    let mut args = std::env::args_os();
    let argv0: PathBuf = args.next().unwrap().into();
    let file_stem = argv0.file_stem().context("argv0 must have a filename")?;
    if file_stem == "spacetimedb-update" {
        spacetimedb_update_main()
    } else if file_stem == "spacetime" {
        proxy::spacetimedb_cli_proxy(Some(argv0.as_os_str()), args.collect())
    } else {
        anyhow::bail!(
            "unknown command name for spacetimedb-update multicall binary: {}",
            Path::new(file_stem).display()
        )
    }
}

#[derive(clap::Parser)]
struct Args {
    #[arg(long)]
    root_dir: Option<RootDir>,
    #[command(subcommand)]
    cmd: Subcommand,
}

#[derive(clap::Subcommand)]
enum Subcommand {
    Version(Version),
    UseVersion(UseVersion),
    Upgrade,
    Install(Install),
    Uninstall(Uninstall),
    #[command(hide = true)]
    Cli {
        #[clap(allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
}

#[derive(clap::Args)]
struct Version {
    #[command(subcommand)]
    subcmd: Option<VersionSubcommand>,
}

#[derive(clap::Subcommand)]
enum VersionSubcommand {
    List,
}

#[derive(clap::Args)]
struct UseVersion {
    #[arg(long)]
    edition: String,
    #[arg(long)]
    version: semver::Version,
}

#[derive(clap::Args)]
struct Install {
    edition: String,
    version: semver::Version,
}

#[derive(clap::Args)]
struct Uninstall {
    edition: String,
    version: semver::Version,
}

fn spacetimedb_update_main() -> anyhow::Result<ExitCode> {
    let args = Args::parse();
    let paths = match &args.root_dir {
        Some(root_dir) => SpacetimePaths::from_root_dir(root_dir),
        None => SpacetimePaths::platform_defaults()?,
    };
    match args.cmd {
        Subcommand::Cli { args: mut cli_args } => {
            if let Some(root_dir) = &args.root_dir {
                cli_args.insert(0, OsString::from_iter(["--root-dir=".as_ref(), root_dir.as_ref()]));
            }
            proxy::run_cli(&paths, None, cli_args)
        }
        _ => unimplemented!(),
    }
}
