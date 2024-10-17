use crate::Config;
use clap::{Arg, ArgMatches, Command};
use reqwest::Url;
use serde::Deserialize;
use webbrowser;

pub fn cli() -> Command {
    Command::new("login")
        .arg(
            Arg::new("host")
                .long("host")
                .required(true)
                .default_value("https://spacetimedb.com")
                .help("Fetch login token from a different host"),
        )
        .about("Login the CLI in to SpacetimeDB")
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct LoginTokenResponse {
    approved: bool,
    session: Option<LoginTokenResponseSession>,
}

#[derive(Deserialize)]
struct LoginTokenResponseSession {
    token: String,
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let remote: &String = args.get_one("host").unwrap();
    // Users like to provide URLs with trailing slashes, which can cause issues due to double-slashes in the routes below.
    let remote = remote.trim_end_matches('/');

    let route = |path| format!("{}{}", remote, path);

    let client = reqwest::Client::new();

    let response: TokenResponse = client
        .get(route("/api/auth/cli/request-login-token"))
        .send()
        .await?
        .json()
        .await?;
    let temp_token = response.token.as_str();

    let browser_url = Url::parse_with_params(route("/login/cli").as_str(), vec![("token", temp_token)])?;
    if webbrowser::open(browser_url.as_str()).is_err() {
        println!("Please open the following URL in your browser: {}", browser_url);
    }

    println!("Waiting to hear response from the server...");
    loop {
        let response: LoginTokenResponse = client
            .get(Url::parse_with_params(
                route("/api/auth/cli/status").as_str(),
                vec![("token", temp_token)],
            )?)
            .send()
            .await?
            .json()
            .await?;
        if response.approved {
            config.set_login_token(response.session.unwrap().token);
            println!("Login successful!");
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    // TODO: test that Ctrl-C returns non-zero, rather than falling through to the Ok(()) here

    Ok(())
}
