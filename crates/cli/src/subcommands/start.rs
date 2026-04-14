use std::ffi::OsString;
use std::io;
use std::process::{Command, ExitCode};

use anyhow::Context;
use clap::{Arg, ArgMatches};
use spacetimedb_paths::SpacetimePaths;

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

pub async fn exec(paths: &SpacetimePaths, args: &ArgMatches) -> anyhow::Result<ExitCode> {
    let edition = args.get_one::<Edition>("edition").unwrap();
    let args = args.get_many::<OsString>("args").unwrap_or_default();
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
        .arg(&paths.cli_config_dir)
        .args(args);

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
