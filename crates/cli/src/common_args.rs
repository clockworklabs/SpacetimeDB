use clap::ArgAction::SetTrue;
use clap::{value_parser, Arg, ValueEnum};

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ClearMode {
    Always,     // parses as "always"
    OnConflict, // parses as "on-conflict"
    Never,      // parses as "never"
}

pub fn server() -> Arg {
    Arg::new("server")
        .long("server")
        .short('s')
        .help("The nickname, host name or URL of the server")
}

pub fn identity() -> Arg {
    Arg::new("identity")
        .long("identity")
        .short('i')
        .help("The identity to use")
}

pub fn anonymous() -> Arg {
    Arg::new("anon_identity")
        .long("anonymous")
        .action(SetTrue)
        .help("Perform this action with an anonymous identity")
}

pub fn yes() -> Arg {
    Arg::new("force")
        .long("yes")
        .short('y')
        .action(SetTrue)
        .help("Run non-interactively wherever possible. This will answer \"yes\" to almost all prompts, but will sometimes answer \"no\" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).")
}

pub fn confirmed() -> Arg {
    Arg::new("confirmed")
        .required(false)
        .long("confirmed")
        .action(SetTrue)
        .help("Instruct the server to deliver only updates of confirmed transactions")
}

pub fn clear_database() -> Arg {
    Arg::new("clear-database")
        .long("delete-data")
        .short('c')
        .num_args(0..=1)
        .value_parser(value_parser!(ClearMode))
        // Because we have a default value for this flag, invocations can be ambiguous between
        //passing a value to this flag, vs using the default value and passing an anonymous arg
        // to the rest of the command. Adding `require_equals` resolves this ambiguity.
        .require_equals(true)
        .default_missing_value("always")
        .help(
            "When publishing to an existing database identity, first DESTROY all data associated with the module. With 'on-conflict': only when breaking schema changes occur."
        )
}
