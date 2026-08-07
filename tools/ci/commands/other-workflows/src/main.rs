#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::Path;

fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}

mod cla_assistant;
mod codeowners_check;
mod watch;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    cmd: OtherWorkflowsCmd,
}
#[derive(Subcommand)]
enum OtherWorkflowsCmd {
    /// Checks that sensitive CODEOWNERS-controlled files have the required approvals.
    CodeownersCheck {
        #[arg(long)]
        base_ref: String,
        #[arg(long)]
        pr_number: u64,
    },
    /// Interacts with CLA Assistant.
    ClaAssistant {
        #[command(subcommand)]
        cmd: cla_assistant::ClaAssistantCmd,
    },
    /// Waits for a GitHub Actions workflow run to complete.
    Watch(watch::Args),
}
fn main() -> Result<()> {
    env_logger::init();
    match Args::parse().cmd {
        OtherWorkflowsCmd::CodeownersCheck { base_ref, pr_number } => codeowners_check::run(&base_ref, pr_number),
        OtherWorkflowsCmd::ClaAssistant { cmd } => cla_assistant::run(cmd),
        OtherWorkflowsCmd::Watch(args) => watch::run(args),
    }
}
