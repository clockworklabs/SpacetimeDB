use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("address")
        .about("???.")
        .override_usage("stdb address <project name>")
        .arg(Arg::new("project name").required(true))
        .after_help("Run `stdb help address for more detailed information.\n`")
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_name = args.value_of("project name").unwrap();

    println!("This is your project name: {}", project_name);
    Ok(())
}
