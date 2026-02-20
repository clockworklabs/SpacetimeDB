use crate::util::decode_identity;
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
    // Check if already logged out.
    if config.spacetimedb_token().is_none() && config.web_session_token().is_none() {
        println!("You are not logged in.");
        return Ok(());
    }

    let host: &String = args.get_one("auth-host").unwrap();
    let host = Url::parse(host)?;

    // Grab identity before clearing tokens.
    let identity = config.spacetimedb_token().and_then(|t| decode_identity(t).ok());

    // Best-effort server-side session invalidation.
    if let Some(web_session_token) = config.web_session_token() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        match client
            .post(host.join("auth/cli/logout")?)
            .header("Authorization", format!("Bearer {web_session_token}"))
            .send()
            .await
        {
            Ok(_) => {}
            Err(_) => {
                eprintln!("Warning: could not reach the server to invalidate your session.");
                eprintln!("Your local credentials have been cleared, but the server-side session may persist.");
            }
        }
    }

    config.clear_login_tokens();
    config.save();

    if let Some(id) = identity {
        println!("Logged out (identity {id}).");
    } else {
        println!("Logged out.");
    }

    Ok(())
}
