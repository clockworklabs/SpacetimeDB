use clap::Command;
use spacetimedb_cli::*;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::load();
    // Save a default version to disk
    config.save();

    let (cmd, subcommand_args) = util::match_subcommand_or_exit(get_command());
    exec_subcommand(config, &cmd, &subcommand_args).await?;

    Ok(())
}

fn get_command() -> Command<'static> {
    Command::new("spacetime")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .help_template(
            "\
┌──────────────────────────────────────────────────────────┐
│ SpacetimeDB Command Line Tool                            │
│ Easily interact with a SpacetimeDB cluster               │
│                                                          │
│ Give us feedback in our Discord server:                  │
│    https://discord.gg/w2DVqNZXdN                         │
└──────────────────────────────────────────────────────────┘
Usage:
{usage}

Options:
{options}

Commands:
{subcommands}
",
        )
}
