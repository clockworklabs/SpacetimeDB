use clap::ArgMatches;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("version")
        .about("Print the version of the command line tool.")
        .after_help("Run `stdb help version for more detailed information.\n`")
}

pub async fn exec(_config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    println!("0.0.0");
    Ok(())
}
