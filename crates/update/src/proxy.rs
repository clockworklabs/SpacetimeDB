use anyhow::Context;
use spacetimedb_paths::{FromPathUnchecked, RootDir, SpacetimePaths};
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitCode;

pub(crate) fn run_cli(
    paths: Option<&SpacetimePaths>,
    argv0: Option<&OsStr>,
    args: Vec<OsString>,
) -> anyhow::Result<ExitCode> {
    let parse_args = || PartialCliArgs::parse(&args);
    let mut is_version_subcommand = None;
    let paths_;
    let paths = match paths {
        Some(paths) => paths,
        None => {
            let partial_args = parse_args()?;
            is_version_subcommand = Some(partial_args.is_version_subcommand());
            paths_ = match partial_args.root_dir {
                Some(root_dir) => SpacetimePaths::from_root_dir(&root_dir),
                None => SpacetimePaths::platform_defaults()?,
            };
            &paths_
        }
    };
    let cli_path = if let Some(artifact_dir) = running_from_target_dir() {
        let cli_path = spacetimedb_paths::cli::VersionBinDir::from_path_unchecked(artifact_dir).spacetimedb_cli();
        anyhow::ensure!(
            cli_path.0.exists(),
            "running spacetimedb-update's cli proxy from a target/ directory, but the
             spacetimedb-cli binary doesn't exist. try running `cargo build -p spacetimedb-cli`"
        );
        cli_path
    } else {
        paths.cli_bin_dir.current_version_dir().spacetimedb_cli()
    };

    let mut cmd = Command::new(&cli_path);
    cmd.args(&args);
    #[cfg(unix)]
    let exec_result = {
        use std::os::unix::process::CommandExt;
        if let Some(argv0) = argv0 {
            cmd.arg0(argv0);
        }
        let err = cmd.exec();
        Err::<std::process::ExitStatus, _>(err)
    };
    #[cfg(windows)]
    let exec_result = cmd.status();
    let exec_err = match exec_result {
        Ok(status) => return Ok(ExitCode::from(status.code().unwrap_or(1).try_into().unwrap_or(1))),
        Err(err) => err,
    };
    // if we failed to exec cli and it seems like the user is trying to run `spacetime version`,
    // patch them through directly.
    if is_version_subcommand.unwrap_or_else(|| parse_args().is_ok_and(|a| a.is_version_subcommand())) {
        return crate::spacetimedb_update_main();
    }
    Err(exec_err)
        .context(format!("exec failed for {}", cli_path.display()))
        .context(
            "It seems like the spacetime version set as current may not exist. Try using `spacetime version`\n\
             to set a different version as default or to install a new version altogether.",
        )
}

/// Checks to see if we're running from a subdirectory of a `target` dir that has a `Cargo.toml`
/// as a sibling, and returns the containing directory of the current executable if so.
pub(crate) fn running_from_target_dir() -> Option<PathBuf> {
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

struct PartialCliArgs<'a> {
    root_dir: Option<RootDir>,
    maybe_subcommand: Option<&'a OsStr>,
}

impl<'a> PartialCliArgs<'a> {
    fn is_version_subcommand(&self) -> bool {
        self.maybe_subcommand.is_some_and(|s| s == "version")
    }

    fn parse(args: &'a [OsString]) -> anyhow::Result<Self> {
        let mut args = args.iter();
        let mut root_dir = None;
        let mut maybe_subcommand = None;
        while let Some(arg) = args.next() {
            let is_arg_value = |s: &OsStr| !os_str_starts_with(arg, "-") || s == "-";
            // "parse" only up to the first subcommand
            if is_arg_value(arg) {
                maybe_subcommand = Some(&**arg);
                break;
            } else if arg == "--" {
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
        Ok(Self {
            root_dir,
            maybe_subcommand,
        })
    }
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