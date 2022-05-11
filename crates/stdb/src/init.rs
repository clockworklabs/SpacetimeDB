use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("init")
        .about("Create a new SpacetimeDB account.")
        .override_usage("stdb init <project name> <path to project>")
        .arg(Arg::new("project name").required(true))
        .arg(Arg::new("path to project").required(true))
        .after_help("Run `stdb help init for more detailed information.\n`")
}

pub fn exec(args: &ArgMatches) {
    let project_name = args.value_of("project name").unwrap();
    let path_to_project = args.value_of("path to project").unwrap();

    println!("This is your project name: {}", project_name);
    println!("This is the path to the project: {}", path_to_project);
}
