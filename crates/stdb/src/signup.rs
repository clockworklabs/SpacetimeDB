use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("signup")
        .about("Create a new SpacetimeDB account.")
        .override_usage("stdb signup <email>")
        .arg(Arg::new("email").required(true))
        .after_help("Run `stdb help signup for more detailed information.\n`")
}


pub fn exec(args: &ArgMatches) {
    let email = args.value_of("email").unwrap();

    println!("This is your email: {}", email);

}
