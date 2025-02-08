#![allow(clippy::disallowed_macros)]

use std::ffi::OsString;
use std::future::Future;
use std::process::ExitCode;

use anyhow::Context;
use spacetimedb_paths::{RootDir, SpacetimePaths};

mod install;
mod link;
mod list;
mod uninstall;
mod upgrade;
mod r#use;

/// Manage installed spacetime versions
#[derive(clap::Parser)]
#[command(bin_name = "spacetime")]
pub struct Args {
    #[arg(long)]
    root_dir: Option<RootDir>,
    #[command(subcommand)]
    cmd: Subcommand,
}

impl Args {
    pub fn exec(self) -> anyhow::Result<ExitCode> {
        let paths = match &self.root_dir {
            Some(root_dir) => SpacetimePaths::from_root_dir(root_dir),
            None => SpacetimePaths::platform_defaults()?,
        };
        match self.cmd {
            Subcommand::Cli { args: mut cli_args } => {
                if let Some(root_dir) = &self.root_dir {
                    cli_args.insert(0, OsString::from_iter(["--root-dir=".as_ref(), root_dir.as_ref()]));
                }
                crate::proxy::run_cli(Some(&paths), None, cli_args)
            }
            Subcommand::Version(version) => version.exec(&paths).map(|()| ExitCode::SUCCESS),
            Subcommand::SelfInstall { install_latest } => {
                let current_exe = std::env::current_exe().context("could not get current exe")?;
                let suppress_eexists = |r: std::io::Result<()>| {
                    r.or_else(|e| (e.kind() == std::io::ErrorKind::AlreadyExists).then_some(()).ok_or(e))
                };
                suppress_eexists(paths.cli_bin_dir.create()).context("could not create bin dir")?;
                suppress_eexists(paths.cli_config_dir.create()).context("could not create config dir")?;
                suppress_eexists(paths.data_dir.create()).context("could not create data dir")?;
                paths
                    .cli_bin_file
                    .create_parent()
                    .and_then(|()| std::fs::copy(&current_exe, &paths.cli_bin_file))
                    .context("could not install binary")?;

                if install_latest {
                    upgrade::Upgrade {}.exec(&paths)?;
                }

                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

#[derive(clap::Subcommand)]
enum Subcommand {
    Version(Version),
    #[command(hide = true)]
    Cli {
        #[clap(allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    SelfInstall {
        /// Download and install the latest CLI version after self-installing.
        #[arg(long)]
        install_latest: bool,
    },
}

#[derive(clap::Args)]
#[command(arg_required_else_help = true)]
struct Version {
    #[command(subcommand)]
    subcmd: VersionSubcommand,
}

impl Version {
    fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        use VersionSubcommand::*;
        match self.subcmd {
            List(subcmd) => subcmd.exec(paths),
            Use(subcmd) => subcmd.exec(paths),
            Upgrade(subcmd) => subcmd.exec(paths),
            Install(subcmd) => subcmd.exec(paths),
            Uninstall(subcmd) => subcmd.exec(paths),
            Link(subcmd) => subcmd.exec(paths),
        }
    }
}

#[derive(clap::Subcommand)]
enum VersionSubcommand {
    List(list::List),
    Use(r#use::Use),
    Upgrade(upgrade::Upgrade),
    Install(install::Install),
    Uninstall(uninstall::Uninstall),
    #[command(hide = true)]
    Link(link::Link),
}

fn reqwest_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("SpacetimeDB CLI/{}", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn tokio_block_on<Fut: Future>(fut: Fut) -> anyhow::Result<Fut::Output> {
    Ok(tokio::runtime::Runtime::new()?.block_on(fut))
}

#[derive(clap::Args)]
struct ForceYes {
    /// Skip the confirmation dialog.
    #[arg(long, short)]
    yes: bool,
}

impl ForceYes {
    fn confirm(self, prompt: String) -> anyhow::Result<bool> {
        let yes = self.yes
            || dialoguer::Confirm::new()
                .with_prompt(prompt)
                .default(false)
                .interact()?;
        Ok(yes)
    }
}
