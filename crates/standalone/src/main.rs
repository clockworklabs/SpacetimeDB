use clap::Command;

use tokio::runtime::Builder;

use spacetimedb_lib::util;
use spacetimedb_standalone::*;
use std::panic;
use std::process;

async fn async_main() -> anyhow::Result<()> {
    let (cmd, subcommand_args) = util::match_subcommand_or_exit(get_command());
    exec_subcommand(&cmd, &subcommand_args).await?;
    Ok(())
}

fn get_command() -> Command {
    Command::new("spacetimedb")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .help_expected(true)
        .help_template(
            r#"
┌──────────────────────────────────────────────────────────┐
│ spacetimedb                                              │
│ Run a standalone SpacetimeDB instance                    │
│                                                          │
│ Please give us feedback at:                              │
│ https://github.com/clockworklabs/SpacetimeDB/issues      │
└──────────────────────────────────────────────────────────┘
Example usage:
┌──────────────────────────────────────────────────────────┐
│ machine# spacetimedb start                               │
└──────────────────────────────────────────────────────────┘
"#,
        )
}

fn main() -> anyhow::Result<()> {
    // Create a multi-threaded run loop
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}
