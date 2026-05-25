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
        /// Directories where NuGet.Config should be written.
        #[arg(required = true)]
        target_dirs: Vec<PathBuf>,
        /// Path to the SpacetimeDB repository whose C# packages should be used.
        #[arg(long, alias = "spacetimedb-repo-path")]
        stdb_path: Option<PathBuf>,
        /// Do not print the generated SDK NuGet.Config contents.
        #[arg(long)]
        quiet: bool,
    },
    /// Run the C# regression test workflow against a running local SpacetimeDB instance.
    RunRegressionTests,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::WriteNugetConfig {
            target_dirs,
            stdb_path,
            quiet,
        } => {
            csharp_tools::write_nuget_configs(&target_dirs, stdb_path.as_deref(), quiet)?;
        }
        Command::RunRegressionTests => {
            csharp_tools::run_regression_tests()?;
        }
    }

    Ok(())
}
