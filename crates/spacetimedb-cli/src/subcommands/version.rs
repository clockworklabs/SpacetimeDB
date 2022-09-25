use clap::ArgMatches;

const CLI_VERSION: &'static str = env!("CARGO_PKG_VERSION");

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("version")
        .about("Print the version of the command line tool.")
        .after_help("Run `spacetime help version for more detailed information.\n`")
}

pub async fn exec(_config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    println!(
        "spacetimedb tool version {}; spacetimedb-lib version {};",
        CLI_VERSION,
        spacetimedb_lib::version::spacetimedb_lib_version()
    );
    Ok(())
}
