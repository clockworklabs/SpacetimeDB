use clap::Command;
use anyhow::Context;
use clap::Parser;
use clap::Subcommand;

use spacetimedb::db::db_metrics;
use spacetimedb::startup;
use spacetimedb::worker_metrics;
use spacetimedb_standalone::routes::router;
use spacetimedb_standalone::StandaloneEnv;
use std::net::TcpListener;
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

fn main() {
    // take_hook() returns the default hook in case when a custom one is not set
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    // Create a multi-threaded run loop
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
        .unwrap();
}
