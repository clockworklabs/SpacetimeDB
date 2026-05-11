use anyhow::Result;
use clap::{Parser, Subcommand};

mod csharp;

#[derive(Parser)]
#[command(name = "regen", bin_name = "cargo regen", about = "Regenerate checked-in artifacts")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Regenerate C# SDK artifacts.
    Csharp {
        #[command(subcommand)]
        command: CsharpCommand,
    },
}

#[derive(Subcommand)]
enum CsharpCommand {
    /// Regenerate C# DLL and NuGet package artifacts for Unity workflows.
    Dlls,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Csharp { command } => match command {
            CsharpCommand::Dlls => csharp::regen_dlls()?,
        },
    }

    Ok(())
}
