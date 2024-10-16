use crate::Config;
use clap::{Arg, ArgMatches, Command};

pub fn cli() -> Command {
    Command::new("login")
        .arg(
          Arg::new("host")
            .long("host")
            .required(true)
            .default_value("https://spacetimedb.com")
            .help("Fetch login token from a different host")
        )
        .about("Login the CLI in to SpacetimeDB")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let remote: &String = args.get_one("host").unwrap();
    // Users like to provide URLs with trailing slashes, which can cause issues due to double-slashes in the routes below.
    let remote = remote.trim_end_matches('/');
    let builder = reqwest::Client::new().get(format!("{}/api/auth/cli/request-login-token", remote));
    let response = builder.send().await?;
    let token = response.error_for_status()?.text().await?;

    poll(/api/auth/cli/status?token=${token})

    openBrowser('/login/cli?token=${token}')

    Ok(())
}
