use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("login")
        .about("Login using an existing identity")
        .override_usage("stdb login <username> <password>")
        .arg(Arg::new("username").required(true))
        .arg(Arg::new("password").required(true))
        .after_help("Run `stdb help login for more detailed information.\n`")
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let username = args.value_of("username").unwrap();
    let _password = args.value_of("password").unwrap();

    println!("This is your username: {}", username);
    Ok(())
}
