use clap::{Args as ClapArgs, Subcommand};

#[derive(ClapArgs)]
pub struct SmoketestsArgs {
    #[command(subcommand)]
    pub cmd: Option<SmoketestCmd>,

    /// Run tests against a remote server instead of spawning local servers.
    ///
    /// When specified, tests will connect to the given URL instead of starting
    /// local server instances. Tests that require local server control (like
    /// restart tests) will be skipped.
    #[arg(long)]
    pub server: Option<String>,

    /// Use a SpacetimeAuth-issued login for remote-server tests.
    ///
    /// This is required for servers that reject direct server-issued logins for privileged operations.
    ///
    /// Optionally accepts an auth host to pass through to `spacetime login`,
    /// for example `--auth-host=https://spacetimedb.com`.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "")]
    pub auth_host: Option<String>,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub dotnet: bool,

    /// Additional arguments to pass to the test runner
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,
}

#[derive(Subcommand)]
pub enum SmoketestCmd {
    /// Only build binaries without running tests
    ///
    /// Use this before running `cargo test --all` to ensure binaries are built.
    Prepare,
    CheckModList,
}
