use anyhow::Context;
use spacetimedb_paths::{FromPathUnchecked, RootDir, SpacetimePaths};
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitCode;

pub(super) fn spacetimedb_cli_proxy(argv0: Option<&OsStr>, args: Vec<OsString>) -> anyhow::Result<ExitCode> {
    let paths = match extract_root_dir_arg(&args)? {
        Some(root_dir) => SpacetimePaths::from_root_dir(&root_dir),
        None => SpacetimePaths::platform_defaults()?,
    };
    run_cli(&paths, argv0, args)
}
pub(crate) fn run_cli(paths: &SpacetimePaths, argv0: Option<&OsStr>, args: Vec<OsString>) -> anyhow::Result<ExitCode> {
    let cli_path = if let Some(artifact_dir) = running_from_target_dir() {
        let cli_path = spacetimedb_paths::cli::VersionBinDir::from_path_unchecked(artifact_dir).spacetimedb_cli();
        anyhow::ensure!(
            cli_path.0.exists(),
            "running spacetimedb-update's cli proxy from a target/ directory, but the
             spacetimedb-cli binary doesn't exist. try running `cargo build -p spacetimedb-cli`"
        );
        cli_path
    } else {
        let version = get_current_version();
        paths.cli_bin_dir.version_dir(version).spacetimedb_cli()
    };
    let mut cmd = Command::new(&cli_path);
    cmd.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        if let Some(argv0) = argv0 {
            cmd.arg0(argv0);
        }
        let err = cmd.exec();
        Err(err).context(format!("exec failed for {}", cli_path.display()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitCodeExt;
        let status = cmd
            .status()
            .with_context(|| format!("failed to run {}", cli_path.display()))?;
        Ok(ExitCode::from_raw(status.code().unwrap_or(1) as u32))
    }
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

fn get_current_version() -> semver::Version {
    // TODO:
    "1.0.0".parse().unwrap()
}

fn extract_root_dir_arg(args: &[OsString]) -> anyhow::Result<Option<RootDir>> {
    let mut args = args.iter();
    let mut root_dir = None;
    while let Some(arg) = args.next() {
        let is_arg_value = |s: &OsStr| !os_str_starts_with(arg, "-") || s == "-";
        // "parse" only up to the first subcommand
        if is_arg_value(arg) || arg == "--" {
            break;
        }
        let root_dir_arg = if arg == "--root-dir" {
            args.next()
                .filter(|s| is_arg_value(s))
                .context("a value is required for '--root-dir <root_dir>' but none was supplied")?
        } else if let Some(arg) = os_str_strip_prefix(arg, "--root-dir=") {
            arg
        } else {
            continue;
        };
        anyhow::ensure!(
            root_dir.is_none(),
            "the argument '--root-dir <root_dir>' cannot be used multiple times"
        );
        root_dir = Some(RootDir(root_dir_arg.into()));
    }
    Ok(root_dir)
}

fn os_str_starts_with(s: &OsStr, pref: &str) -> bool {
    s.as_encoded_bytes().starts_with(pref.as_bytes())
}

fn os_str_strip_prefix<'a>(s: &'a OsStr, pref: &str) -> Option<&'a OsStr> {
    os_str_starts_with(s, pref).then(|| {
        // SAFETY: we're splitting immediately after a valid non-empty UTF-8 substring.
        //         (if pref is empty then this is a no-op and trivially safe)
        unsafe { OsStr::from_encoded_bytes_unchecked(&s.as_encoded_bytes()[pref.len()..]) }
    })
}
