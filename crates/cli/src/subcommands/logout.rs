use crate::util::decode_identity;
use crate::Config;
use clap::{Arg, ArgMatches, Command};
use reqwest::Url;
use std::time::Duration;

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

    let _ = ensure_logged_out(&mut config, &host).await;

    Ok(())
}

async fn server_logout(config: &mut Config, host: &Url) -> Result<(), anyhow::Error> {
    let Some(web_session_token) = config.web_session_token() else {
        anyhow::bail!("No web session token");
    };
    // Best-effort server-side session invalidation.
    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build()?;
    client
        .post(host.join("auth/cli/logout")?)
        .header("Authorization", format!("Bearer {web_session_token}"))
        .send()
        .await?;
    Ok(())
}

/// Logs out the user from the specified auth server.
/// Returns true if the user was logged out, false if they were not logged in.
pub async fn ensure_logged_out(config: &mut Config, host: &Url) -> bool {
    let Some(token) = config.spacetimedb_token() else {
        return false;
    };
    // Grab identity before clearing tokens.
    let identity = decode_identity(token).ok();

    // Best-effort server-side session invalidation.
    if let Err(e) = server_logout(config, host).await {
        eprintln!("Warning: Failed to logout from auth server: {e}\nLocal credentials have been cleared.");
    }
    config.clear_login_tokens();
    config.save();

    if let Some(id) = identity {
        println!("Logged out (identity {id}).");
    } else {
        println!("Logged out.");
    }

    true
}
