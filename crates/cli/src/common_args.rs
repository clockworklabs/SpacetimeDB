use clap::Arg;
use clap::ArgAction::SetTrue;

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
