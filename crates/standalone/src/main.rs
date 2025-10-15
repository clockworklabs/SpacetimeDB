use clap::Command;

use spacetimedb::startup;
use spacetimedb::util::jobs::JobCores;
use tokio::runtime::Builder;

use spacetimedb_standalone::*;
use std::panic;
use std::process;

async fn async_main(db_cores: JobCores) -> anyhow::Result<()> {
    let matches = get_command().get_matches();
    let (cmd, subcommand_args) = matches.subcommand().unwrap();
    exec_subcommand(cmd, subcommand_args, db_cores).await?;
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

// Defaults our jemalloc configuration to allow for profiling, but have it disabled initially.
// It can be enabled with an internal endpoint.
// When enabled, it will sample once per 2^19 bytes (512 KB) of memory allocated.

// This can be overridden by setting the `_RJEM_MALLOC_CONF` environment variable.
// We export this symbol so that the jemalloc library can find it.
// See https://github.com/polarsignals/rust-jemalloc-pprof?tab=readme-ov-file#usage
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
pub static _rjem_malloc_conf: &[u8] = b"prof:true,prof_active:false,lg_prof_sample:19\0";

fn main() -> anyhow::Result<()> {
    // take_hook() returns the default hook in case when a custom one is not set
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    let cores = startup::pin_threads();

    // Create a multi-threaded run loop
    let mut builder = Builder::new_multi_thread();
    builder.enable_all();
    cores.tokio.configure(&mut builder);
    let rt = builder.build().unwrap();
    cores.rayon.configure(rt.handle());
    let database_cores = cores.databases.make_database_runners(rt.handle());

    // Keep a handle on the `database_cores` alive outside of `async_main`
    // and explicitly drop it to avoid dropping it from an `async` context -
    // Tokio gets angry when you drop a runtime within another runtime.
    let res = rt.block_on(async_main(database_cores.clone()));
    drop(database_cores);

    res
}
