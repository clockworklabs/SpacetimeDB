use clap::Arg;

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
