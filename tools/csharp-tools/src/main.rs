use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "csharp-tools", bin_name = "cargo csharp", about = "C# SDK maintenance tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write NuGet.Config files that point at local SpacetimeDB C# packages.
    WriteNugetConfig {
        /// Path to the SpacetimeDB repository whose C# packages should be used.
        spacetimedb_repo_path: Option<PathBuf>,
    },
    /// Run the C# regression test workflow against a running local SpacetimeDB instance.
    RunRegressionTests,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::WriteNugetConfig { spacetimedb_repo_path } => {
            csharp_tools::write_persistent_nuget_configs(spacetimedb_repo_path.as_deref())?;
        }
        Command::RunRegressionTests => {
            csharp_tools::run_regression_tests()?;
        }
    }

    Ok(())
}
