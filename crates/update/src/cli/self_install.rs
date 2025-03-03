use std::process::ExitCode;

use anyhow::Context;
use spacetimedb_paths::{RootDir, SpacetimePaths};

use crate::cli::ForceYes;

/// Install the SpacetimeDB CLI.
// NOTICE: If you change anything here, please make the same changes in spacetime-install.sh
#[derive(clap::Parser)]
#[command(bin_name = "spacetime-install[EXE]")]
pub struct SelfInstall {
    /// The directory to locally install SpacetimeDB into. If unspecified, uses platform defaults.
    #[arg(long)]
    root_dir: Option<RootDir>,

    #[command(flatten)]
    yes: ForceYes,
}

impl SelfInstall {
    pub fn exec(self) -> anyhow::Result<ExitCode> {
        let paths = match &self.root_dir {
            Some(root_dir) => SpacetimePaths::from_root_dir(root_dir),
            None => SpacetimePaths::platform_defaults()?,
        };

        let SpacetimePaths {
            cli_config_dir,
            cli_bin_file,
            cli_bin_dir,
            data_dir,
        } = &paths;

        let root_dir = self.root_dir.or_else(|| paths.to_root_dir());
        eprint!("The SpacetimeDB command line tool will now be installed");
        if let Some(root_dir) = &root_dir {
            eprintln!(" into {}", root_dir.display());
        } else {
            eprintln!(":");
            eprintln!("\tCLI configuration directory: {}", cli_config_dir.display());
            eprintln!("\t`spacetime` binary: {}", cli_bin_file.display());
            eprintln!(
                "\tdirectory for installed SpacetimeDB versions: {}",
                cli_bin_dir.display()
            );
            eprintln!("\tdatabase directory: {}", data_dir.display());
        }
        if !self
            .yes
            .confirm_with_default("Would you like to continue?".to_owned(), true)?
        {
            eprintln!("Exiting.");
            return Ok(ExitCode::FAILURE);
        }

        let current_exe = std::env::current_exe().context("could not get current exe")?;
        let suppress_eexists = |r: std::io::Result<()>| {
            r.or_else(|e| (e.kind() == std::io::ErrorKind::AlreadyExists).then_some(()).ok_or(e))
        };
        suppress_eexists(cli_bin_dir.create()).context("could not create bin dir")?;
        suppress_eexists(cli_config_dir.create()).context("could not create config dir")?;
        suppress_eexists(data_dir.create()).context("could not create data dir")?;
        cli_bin_file
            .create_parent()
            .and_then(|()| std::fs::copy(&current_exe, cli_bin_file))
            .context("could not install binary")?;

        eprintln!("Downloading latest version...");
        let res = super::upgrade::Upgrade {}
            .exec(&paths)
            .context("failed to download and install latest SpacetimeDB version");
        if let Err(err) = &res {
            eprintln!("Error: {err:#}\n")
        }

        eprintln!(
            "The `spacetime` command has been installed as {}",
            cli_bin_file.display()
        );
        eprintln!();

        if cfg!(unix) {
            let path_var = std::env::var_os("PATH").unwrap_or_default();
            let bin_dir = cli_bin_file.0.parent().unwrap();
            if !std::env::split_paths(&path_var).any(|p| p == bin_dir) {
                eprintln!(
                    "\
It seems like this directory is not in your `PATH` variable. Please add the
following line to your shell configuration and open a new shell session:

    export PATH=\"{}:$PATH\"
",
                    bin_dir.display()
                )
            }
        }

        eprintln!(
            "\
The install process is complete; check out our quickstart guide to get started!
	<https://spacetimedb.com/docs/quick-start>"
        );

        Ok(ExitCode::SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn ensure_script_help_is_up_to_date() {
        let help_text = SelfInstall::command().term_width(80).render_long_help().to_string();
        let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/spacetime-install.sh")).unwrap();
        let (_, heredoc) = script
            .split_once("usage() {\n    cat <<EOF\n")
            .expect("couldn't find usage function");
        let (heredoc, _) = heredoc.split_once("EOF").expect("couldn't find end of heredoc");
        assert!(
            help_text == heredoc,
            "the usage text in spacetime-install.sh is out of date from the CLI. it should be:\n{help_text}"
        );
    }
}
