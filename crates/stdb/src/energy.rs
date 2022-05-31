// use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("energy")
        .about("Invokes commands related to energy.")
        .override_usage("stdb energy [buy|info] [OPTIONS]")
        // .arg(Arg::new("").required(true))
        .after_help("Run `stdb help energy for more detailed information.\n`")
}

pub async fn exec(_args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();

    // println!("This is your project_name: {}", project_name);
    Ok(())
}
