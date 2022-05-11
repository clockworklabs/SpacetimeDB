use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("call")
        .about("Invokes a SpacetimeDB function.")
        .override_usage("stdb call <project name> <function name> -- <function params as json>")
        .arg(Arg::new("project name").required(true))
        .arg(Arg::new("function name").required(true))
        .after_help("Run `stdb help call for more detailed information.\n`")
}

pub fn exec(args: &ArgMatches) {
    let project_name = args.value_of("project_name").unwrap();
    let function_name = args.value_of("function_name").unwrap();

    println!("This is your project_name: {}", project_name);
    println!("This is your function_name: {}", function_name);
}
