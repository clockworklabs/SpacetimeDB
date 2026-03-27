use std::ffi::OsString;
use std::io;
use std::process::{Command, ExitCode};

use anyhow::{anyhow, Context};
use clap::{Arg, ArgMatches};
use spacetimedb_paths::SpacetimePaths;

use crate::spacetime_config::find_and_load_with_env;
use crate::util::resolve_sibling_binary;

pub fn cli() -> clap::Command {
    clap::Command::new("start")
        .about("Start a local SpacetimeDB instance")
        .long_about(
            "\
Start a local SpacetimeDB instance

Run `spacetime start --help` to see all options.",
        )
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

/// Check whether the forwarded args already contain `--listen-addr` or `-l`.
///
/// Handles all common forms:
/// - `--listen-addr <value>` (two separate tokens)
/// - `--listen-addr=<value>`
/// - `-l <value>` (two separate tokens)
/// - `-l<value>` (short flag with attached value, e.g. `-l0.0.0.0:4000`)
fn has_listen_addr_arg(args: impl Iterator<Item = impl AsRef<std::ffi::OsStr>>) -> bool {
    for arg in args {
        let s = arg.as_ref().to_string_lossy();
        // --listen-addr or --listen-addr=<value>
        if s == "--listen-addr" || s.starts_with("--listen-addr=") {
            return true;
        }
        // Exactly `-l` (value in next token) or `-l` followed by a non-alphabetic
        // char (attached value like `-l0.0.0.0:4000`). This avoids false positives
        // on hypothetical flags like `-log` while correctly matching the `-l` short
        // flag for `--listen-addr`.
        if s == "-l"
            || (s.starts_with("-l")
                && !s.starts_with("--")
                && s.as_bytes().get(2).is_some_and(|b| !b.is_ascii_alphabetic()))
        {
            return true;
        }
    }
    false
}

/// Resolve the listen address from config (`spacetime.json`).
///
/// Returns `Some(addr)` if a `listen-addr` key is found in the project config,
/// or `None` if no config file exists or the key is absent.
fn resolve_listen_addr_from_config() -> anyhow::Result<Option<String>> {
    let Some(loaded) = find_and_load_with_env(None)? else {
        return Ok(None);
    };
    let Some(value) = loaded.config.additional_fields.get("listen-addr") else {
        return Ok(None);
    };

    let listen_addr = value
        .as_str()
        .ok_or_else(|| anyhow!("invalid `listen-addr` in spacetime.json: expected a string, got {value}"))?
        .to_owned();

    Ok(Some(listen_addr))
}

pub async fn exec(paths: &SpacetimePaths, args: &ArgMatches) -> anyhow::Result<ExitCode> {
    let edition = args.get_one::<Edition>("edition").unwrap();
    let forwarded_args: Vec<OsString> = args.get_many::<OsString>("args").unwrap_or_default().cloned().collect();
    let bin_name = match edition {
        Edition::Standalone => "spacetimedb-standalone",
        Edition::Cloud => "spacetimedb-cloud",
    };
    let bin_path = resolve_sibling_binary(bin_name)?;
    let mut cmd = Command::new(&bin_path);
    cmd.arg("start")
        .arg("--data-dir")
        .arg(&paths.data_dir)
        .arg("--jwt-key-dir")
        .arg(&paths.cli_config_dir);

    // Resolve listen-addr with precedence: CLI > config > built-in default.
    // If the user already passed --listen-addr / -l in the forwarded args, pass
    // everything through unchanged. Otherwise, check spacetime.json for a
    // configured default and inject it.
    if !has_listen_addr_arg(forwarded_args.iter())
        && let Some(config_addr) = resolve_listen_addr_from_config()?
    {
        cmd.arg("--listen-addr").arg(&config_addr);
    }

    cmd.args(&forwarded_args);

    exec_replace(&mut cmd).with_context(|| format!("exec failed for {}", bin_path.display()))
}

// implementation based on and docs taken verbatim from `cargo_util::ProcessBuilder::exec_replace`
//
/// Replaces the current process with the target process.
///
/// On Unix, this executes the process using the Unix syscall `execvp`, which will block
/// this process, and will only return if there is an error.
///
/// On Windows this isn't technically possible. Instead we emulate it to the best of our
/// ability. One aspect we fix here is that we specify a handler for the Ctrl-C handler.
/// In doing so (and by effectively ignoring it) we should emulate proxying Ctrl-C
/// handling to the application at hand, which will either terminate or handle it itself.
/// According to Microsoft's documentation at
/// <https://docs.microsoft.com/en-us/windows/console/ctrl-c-and-ctrl-break-signals>.
/// the Ctrl-C signal is sent to all processes attached to a terminal, which should
/// include our child process. If the child terminates then we'll reap them in Cargo
/// pretty quickly, and if the child handles the signal then we won't terminate
/// (and we shouldn't!) until the process itself later exits.
pub(crate) fn exec_replace(cmd: &mut Command) -> io::Result<ExitCode> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // if exec() succeeds, it diverges, so the function just returns an io::Error
        let err = cmd.exec();
        Err(err)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{BOOL, FALSE, TRUE};
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

        unsafe extern "system" fn ctrlc_handler(_: u32) -> BOOL {
            // Do nothing. Let the child process handle it.
            TRUE
        }
        unsafe {
            if SetConsoleCtrlHandler(Some(ctrlc_handler), TRUE) == FALSE {
                return Err(io::Error::other("Unable to set console handler"));
            }
        }

        cmd.status()
            .map(|status| ExitCode::from(status.code().unwrap_or(1).try_into().unwrap_or(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── has_listen_addr_arg tests ──────────────────────────────────────

    #[test]
    fn detects_long_flag_separate_value() {
        assert!(has_listen_addr_arg(["--listen-addr", "0.0.0.0:4000"].iter()));
    }

    #[test]
    fn detects_long_flag_equals_value() {
        assert!(has_listen_addr_arg(["--listen-addr=0.0.0.0:4000"].iter()));
    }

    #[test]
    fn detects_short_flag_separate_value() {
        assert!(has_listen_addr_arg(["-l", "0.0.0.0:4000"].iter()));
    }

    #[test]
    fn detects_short_flag_attached_value() {
        assert!(has_listen_addr_arg(["-l0.0.0.0:4000"].iter()));
    }

    #[test]
    fn detects_short_flag_attached_ipv6() {
        assert!(has_listen_addr_arg(["-l[::1]:4000"].iter()));
    }

    #[test]
    fn ignores_unrelated_long_flag() {
        assert!(!has_listen_addr_arg(["--data-dir", "/tmp"].iter()));
    }

    #[test]
    fn ignores_unrelated_short_flag() {
        assert!(!has_listen_addr_arg(["-d", "/tmp"].iter()));
    }

    #[test]
    fn no_false_positive_on_hyphen_l_prefix_flag() {
        // A hypothetical flag like `-log` should not be detected.
        assert!(!has_listen_addr_arg(["-log"].iter()));
    }

    #[test]
    fn no_false_positive_on_hyphen_li() {
        assert!(!has_listen_addr_arg(["-li"].iter()));
    }

    #[test]
    fn returns_false_for_empty() {
        let empty: Vec<&str> = vec![];
        assert!(!has_listen_addr_arg(empty.iter()));
    }

    #[test]
    fn detects_among_many_args() {
        assert!(has_listen_addr_arg(
            ["--data-dir", "/tmp", "--listen-addr", "0.0.0.0:4000", "--in-memory"].iter()
        ));
    }

    #[test]
    fn detects_short_among_many_args() {
        assert!(has_listen_addr_arg(
            ["--data-dir", "/tmp", "-l", "127.0.0.1:5000"].iter()
        ));
    }
}
