use clap::Command;

use tokio::runtime::Builder;

use spacetimedb_standalone::*;
use std::panic;
use std::process;

async fn async_main() -> anyhow::Result<()> {
    let matches = get_command().get_matches();
    let (cmd, subcommand_args) = matches.subcommand().unwrap();
    exec_subcommand(cmd, subcommand_args).await?;
    Ok(())
}

fn get_command() -> Command {
    Command::new("spacetimedb")
        .args_conflicts_with_subcommands(true)
        .arg_required_else_help(true)
        .version(version::CLI_VERSION)
        .long_version(version::long_version())
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

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() -> anyhow::Result<()> {
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
}
