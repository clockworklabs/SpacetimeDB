use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("revert")
        .about("Reverts the database to a given point in time.")
        .override_usage("stdb revert <project name> <timestamp | commit hash>")
        .arg(Arg::new("project name").required(true))
        .arg(Arg::new("timestamp").required(true))
        .after_help("Run `stdb help revert for more detailed information.\n`")
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_name = args.value_of("project name").unwrap();
    let timestamp_or_hash = args.value_of("timestamp").unwrap();

    println!("This is your project name: {}", project_name);
    println!("This is your timestamp: {}", timestamp_or_hash);
    Ok(())
}
