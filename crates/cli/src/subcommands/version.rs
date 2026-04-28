use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::Context;
use clap::{ArgMatches, Args};
use spacetimedb_paths::cli::BinFile;
use spacetimedb_paths::{FromPathUnchecked, RootDir, SpacetimePaths};

pub fn cli() -> clap::Command {
    Version::augment_args(clap::Command::new("version"))
}

/// Manage installed spacetime versions
///
/// Run `spacetime version --help` to see all options.
#[derive(clap::Args)]
#[command(disable_help_flag = true)]
struct Version {
    /// The args to pass to spacetimedb-update
    #[arg(allow_hyphen_values = true, num_args = 0..)]
    args: Vec<OsString>,
}

pub async fn exec(paths: &SpacetimePaths, root_dir: Option<&RootDir>, args: &ArgMatches) -> anyhow::Result<ExitCode> {
    let args = args.get_many::<OsString>("args").unwrap_or_default();
    let bin_path;
    let bin_path = if let Some(artifact_dir) = running_from_target_dir() {
        let update_path = artifact_dir
            .join("spacetimedb-update")
            .with_extension(std::env::consts::EXE_EXTENSION);
        anyhow::ensure!(
            update_path.exists(),
            "running `spacetime version` from a target/ directory, but the spacetimedb-update
             binary doesn't exist. try running `cargo build -p spacetimedb-update`"
        );
        bin_path = BinFile::from_path_unchecked(update_path);
        &bin_path
    } else {
        &paths.cli_bin_file
    };
    let mut cmd = Command::new(bin_path);
    if let Some(root_dir) = root_dir {
        cmd.arg("--root-dir").arg(root_dir);
    }
    cmd.arg("version").args(args);
    let applet = "spacetimedb-update";
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.arg0(applet);
    }
    #[cfg(windows)]
    cmd.env("SPACETIMEDB_UPDATE_MULTICALL_APPLET", applet);
    super::start::exec_replace(&mut cmd).with_context(|| format!("exec failed for {}", bin_path.display()))
}

/// Checks to see if we're running from a subdirectory of a `target` dir that has a `Cargo.toml`
/// as a sibling, and returns the containing directory of the current executable if so.
fn running_from_target_dir() -> Option<PathBuf> {
    let mut exe_path = std::env::current_exe().ok()?;
    exe_path.pop();
    let artifact_dir = exe_path;
    // check for target/debug/spacetimedb-update and target/x86_64-unknown-foobar/debug/spacetimedb-update
    let target_dir = artifact_dir
        .ancestors()
        .skip(1)
        .take(2)
        .find(|p| p.file_name() == Some("target".as_ref()))?;
    target_dir
        .parent()?
        .join("Cargo.toml")
        .try_exists()
        .ok()
        .map(|_| artifact_dir)
}
