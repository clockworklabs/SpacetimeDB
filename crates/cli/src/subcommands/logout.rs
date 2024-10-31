use crate::Config;
use clap::{Arg, ArgMatches, Command};
use reqwest::Url;

pub fn cli() -> Command {
    Command::new("logout").arg(
        Arg::new("auth-host")
            .long("auth-host")
            .default_value("https://spacetimedb.com")
            .help("Log out from a custom auth server"),
    )
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let host: &String = args.get_one("auth-host").unwrap();
    let host = Url::parse(host)?;

    if let Some(web_session_id) = config.web_session_id() {
        let client = reqwest::Client::new();
        client
            .post(host.join("auth/cli/logout")?)
            .header("Authorization", format!("Bearer {}", web_session_id))
            .send()
            .await?;
    }

    config.clear_login_tokens();
    config.save();

    Ok(())
}
