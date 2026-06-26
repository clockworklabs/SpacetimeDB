use clap::Parser;
use spacetimedb_dst::cli::{Cli, Command};
use spacetimedb_dst::{cli, init_tracing};

fn main() -> anyhow::Result<()> {
    init_tracing();
    match Cli::parse().command {
        Command::Run(args) => cli::run_command_blocking(args),
    }
}
