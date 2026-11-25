//! `cargo llm-apps`: runs the LLM app-building benchmarks.
//!
//! - `oneshot`: the one-shot benchmark in `tools/llm-oneshot`, where a model builds an app
//!   from a single prompt inside Cursor.
//! - `sequential`: the sequential-upgrade benchmark in `tools/llm-sequential-upgrade`, where
//!   headless Claude Code builds an app and then upgrades it level by level.
//!
//! Generated apps and run output never live in this repository. They are published to
//! `clockworklabs/spacetimedb-llm-oneshot-results` and
//! `clockworklabs/spacetimedb-llm-sequential-upgrade-results`.

#![allow(clippy::disallowed_macros)] // CLI output

use anyhow::Result;
use clap::{Parser, Subcommand};

mod format;
mod oneshot;
mod paths;
mod process;
mod results_repo;
mod sequential;

#[derive(Parser)]
#[command(
    name = "llm-apps",
    bin_name = "cargo llm-apps",
    about = "Run the LLM app-building benchmarks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The one-shot benchmark (tools/llm-oneshot).
    Oneshot {
        #[command(subcommand)]
        command: oneshot::Command,
    },
    /// The sequential-upgrade benchmark (tools/llm-sequential-upgrade).
    Sequential {
        #[command(subcommand)]
        command: sequential::Command,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Oneshot { command } => oneshot::run(command),
        Command::Sequential { command } => sequential::run(command),
    }
}
