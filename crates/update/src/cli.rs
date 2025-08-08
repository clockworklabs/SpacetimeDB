#![allow(clippy::disallowed_macros)]

use std::ffi::OsString;
use std::future::Future;
use std::process::ExitCode;

use spacetimedb_paths::{RootDir, SpacetimePaths};

mod install;
mod link;
mod list;
mod self_install;
mod uninstall;
mod upgrade;
mod r#use;

pub use self_install::SelfInstall;

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
            Subcommand::SelfInstall(self_install) => self_install.exec(),
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
    SelfInstall(SelfInstall),
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
    let mut client = reqwest::Client::builder();
    #[cfg(feature = "github-token-auth")]
    {
        use reqwest::header;
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            eprintln!("HTTP requests will use the GITHUB_TOKEN from your environment");
            let mut headers = header::HeaderMap::new();
            headers.insert(header::AUTHORIZATION, format!("Bearer {}", token).parse().unwrap());
            client = client.default_headers(headers);
        }
    }
    client = client.user_agent(format!("SpacetimeDB CLI/{}", env!("CARGO_PKG_VERSION")));
    Ok(client.build()?)
}

fn tokio_block_on<Fut: Future>(fut: Fut) -> anyhow::Result<Fut::Output> {
    Ok(tokio::runtime::Runtime::new()?.block_on(fut))
}

#[derive(clap::Args, Copy, Clone)]
struct ForceYes {
    /// Skip the confirmation dialog.
    #[arg(long, short)]
    yes: bool,
}

impl ForceYes {
    fn confirm_with_default(self, prompt: String, default: bool) -> anyhow::Result<bool> {
        let yes = self.yes
            || dialoguer::Confirm::new()
                .with_prompt(prompt)
                .default(default)
                .interact()?;
        Ok(yes)
    }
    fn confirm(self, prompt: String) -> anyhow::Result<bool> {
        self.confirm_with_default(prompt, false)
    }
}
