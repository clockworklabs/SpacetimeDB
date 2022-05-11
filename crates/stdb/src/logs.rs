use clap::Arg;
use clap::ArgMatches;
use clap::Parser;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database.")
        .override_usage("stdb logs -f <project name>")
        .arg(Arg::new("project name").required(true))
        .after_help("Run `stdb help logs for more detailed information.\n`")
}


pub fn exec(args: &ArgMatches) {
    let project_name = args.value_of("project name").unwrap();

    println!("This is your project_name: {}", project_name);

}
