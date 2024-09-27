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

pub fn yes() -> Arg {
    Arg::new("force")
        .long("yes")
        .action(SetTrue)
        .help("Assume \"yes\" as answer to all prompts and run non-interactively")
}
