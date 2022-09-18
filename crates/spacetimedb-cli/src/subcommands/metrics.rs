use anyhow::Ok;
use clap::Arg;
use clap::ArgMatches;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("metrics")
        .about("Prints metrics.")
        .override_usage("stdb metrics <project name>")
        .arg(Arg::new("project name").required(true))
        .after_help("Run `stdb help metrics for more detailed information.\n`")
}

pub async fn exec(_config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_name = args.value_of("project name").unwrap();

    println!("This is your project name: {}", project_name);
    Ok(())
}
