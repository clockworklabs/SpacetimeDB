//! One invocation owns one exec socket. A lost connection is never replayed.
#[path = "exec/session.rs"]
mod session;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "exec/terminal.rs"]
mod terminal;
#[cfg(windows)]
#[path = "exec/terminal_windows.rs"]
mod terminal;
#[cfg(test)]
#[path = "exec/tests.rs"]
mod tests;

use anyhow::{ensure, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use spacetimedb_lib::container::{
    exec::ExecStart,
    operations::{ContainerStatus, DesiredState, ObservedState},
};
use std::collections::BTreeMap;

pub(super) fn cli() -> Command {
    Command::new("exec")
        .about("Run a literal command in the current running container")
        .after_help("Requires database Admin permission. No shell, container start, or reconnect is implicit. Use -- before COMMAND; for a shell, name its executable explicitly. Linux, macOS, and Windows terminals are supported. Windows requires an attached VT-capable console for --tty; inherited asynchronous seekable files are unsupported. A lost connection does not establish that the process exited.")
        .arg(Arg::new("database").required(true).help("Database name or Identity"))
        .arg(crate::common_args::server())
        .arg(crate::common_args::yes())
        .arg(Arg::new("interactive").short('i').long("interactive").action(ArgAction::SetTrue).help("Forward stdin and send EOF when it closes"))
        .arg(Arg::new("tty").short('t').long("tty").requires("interactive").action(ArgAction::SetTrue).help("Allocate a PTY using this foreground terminal's dimensions"))
        .arg(Arg::new("workdir").long("workdir").help("Absolute working directory inside the container"))
        .arg(Arg::new("environment").short('e').long("env").action(ArgAction::Append).value_name("NAME=VALUE").help("Override a process environment variable; platform keys are reserved"))
        .arg(Arg::new("argv").required(true).num_args(1..).last(true).value_name("COMMAND").help("Executable and literal arguments; no shell expansion"))
}

fn start(args: &ArgMatches, generation: u64) -> Result<ExecStart> {
    let mut environment = BTreeMap::new();
    for value in args.get_many::<String>("environment").into_iter().flatten() {
        let (key, value) = value
            .split_once('=')
            .context("exec environment overrides require NAME=VALUE")?;
        ensure!(
            environment.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate exec environment override"
        );
    }
    let start = ExecStart {
        generation,
        argv: args
            .get_many::<String>("argv")
            .context("exec command is required")?
            .cloned()
            .collect(),
        working_directory: args.get_one::<String>("workdir").cloned(),
        environment,
        stdin: args.get_flag("interactive"),
        terminal: None,
    };
    start
        .validate()
        .context("invalid exec command, directory, or environment override")?;
    Ok(start)
}

fn running_generation(status: &ContainerStatus) -> Result<u64> {
    ensure!(status.published, "database has no published container");
    let operation = status.operational.as_ref().context("container is not running")?;
    let instance = operation
        .current_instance
        .as_ref()
        .context("container is not running")?;
    ensure!(
        operation.desired_state == DesiredState::Running
            && operation.generation != 0
            && instance.generation == operation.generation
            && matches!(instance.state, ObservedState::Running | ObservedState::Ready),
        "container is not running at the current generation"
    );
    Ok(operation.generation)
}

pub(super) async fn exec(config: &mut crate::Config, args: &ArgMatches) -> Result<()> {
    // Reject unsupported terminal implementations before login, status, or a socket.
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (config, args);
        anyhow::bail!("container exec terminal support is not implemented on this host platform");
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    {
        let mut start = start(args, 1)?;
        if args.get_flag("tty") {
            start.terminal = Some(terminal::dimensions()?);
        }
        // Classify inherited Windows handles before any login or network I/O.
        #[cfg(windows)]
        let prepared = terminal::Prepared::stdio(start.stdin, start.terminal.is_some())?;
        let selection = args.get_one::<String>("server").map(String::as_str);
        let origin = crate::container::publish::client::endpoint(&config.get_host_url(selection)?)?;
        let auth = crate::util::get_auth_header(config, false, selection, !args.get_flag("force")).await?;
        let mut auth = auth.to_header().context("container exec requires a login")?;
        auth.set_sensitive(true);
        let client = super::operations::ContainerClient::new(origin, auth.clone())?;
        let status = client
            .status(args.get_one::<String>("database").context("database is required")?)
            .await?;
        start.generation = running_generation(&status)?;
        let url = client.url(status.database_identity.to_hex().as_ref(), "exec")?;
        #[cfg(not(windows))]
        let signals = terminal::signals(start.terminal.is_some())?;
        #[cfg(not(windows))]
        let (mut terminal, mut io) = terminal::Terminal::stdio(start.stdin, start.terminal.is_some())?;
        #[cfg(windows)]
        let (mut terminal, mut io) = prepared.start()?;
        #[cfg(windows)]
        let signals = terminal.signals()?;
        let result = session::run(url, auth, status.database_identity, start, &mut io, signals).await;
        // Always join and restore the terminal, including a failed handshake.
        let cleanup = terminal.finish();
        let code = match (result, cleanup) {
            (Ok(code), Ok(())) => code,
            (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
            (Err(error), Err(cleanup)) => return Err(error.context(format!("terminal cleanup also failed: {cleanup}"))),
        };
        if code != 0 {
            return Err(crate::ExitWithCode(std::process::ExitCode::from(code)).into());
        }
        Ok(())
    }
}
