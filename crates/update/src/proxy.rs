use anyhow::Context;
use spacetimedb_paths::RootDir;
use spacetimedb_paths::SpacetimePaths;
use std::ffi::OsStr;
use std::ffi::OsString;
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
    let version = get_current_version();
    let cli_path = paths.cli_bin_dir.version_dir(version).spacetimedb_cli();
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
