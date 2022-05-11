

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("signup")
        .about("Create a new SpacetimeDB account.")
        .override_usage("stdb add <email>")
        .group(clap::ArgGroup::new("email group").multiple(true).required(true))
        .args([
              clap::Arg::new("email")
              .takes_value(true)
              .value_name("EMAIL_VALUE")
              .multiple_occurrences(false)
              .help("Email address to register")
        ])
}

pub fn exec() {

}
