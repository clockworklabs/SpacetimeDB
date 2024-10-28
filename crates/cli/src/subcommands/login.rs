use crate::util::decode_identity;
use crate::Config;
use clap::{Arg, ArgMatches, Command};
use reqwest::Url;
use serde::Deserialize;
use webbrowser;

pub fn cli() -> Command {
    Command::new("login")
        .args_conflicts_with_subcommands(true)
        .subcommands(get_subcommands())
        .arg(
            Arg::new("auth-host")
                .long("auth-host")
                .default_value("https://spacetimedb.com")
                .help("Fetch login token from a different host"),
        )
        .arg(
            Arg::new("server")
                .long("direct")
                .conflicts_with("auth-host")
                .help("Log in to a SpacetimeDB server directly, without going through a global auth server"),
        )
        .arg(
            Arg::new("spacetimedb-token")
                .long("token")
                .conflicts_with("auth-host")
                .help("Bypass the login flow and use a login token directly"),
        )
        .about("Log the CLI in to SpacetimeDB")
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("show")
        .arg(
            Arg::new("token")
                .long("token")
                .action(ArgAction::SetTrue)
                .help("Also show the auth token"),
        )
        .about("Show the current login info"),
        Command::new("clear")
        .arg(
            Arg::new("token")
                .long("token")
                .action(ArgAction::SetTrue)
                .help("Also show the auth token"),
        )
        .about("Show the current login info")],
    ]
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    if let Some((cmd, subcommand_args)) = args.subcommand() {
        return exec_subcommand(config, cmd, subcommand_args).await;
    }

    let spacetimedb_token: Option<&String> = args.get_one("spacetimedb-token");
    let host: &String = args.get_one("auth-host").unwrap();
    let direct_login: Option<&String> = args.get_one("server");

    if let Some(token) = spacetimedb_token {
        config.set_spacetimedb_token(token.clone());
        config.save();
        return Ok(());
    }

    if let Some(server) = direct_login {
        let host = config.get_host_url(Some(server))?;
        spacetimedb_token_cached(&mut config, &host, true).await?;
    } else {
        spacetimedb_token_cached(&mut config, host, false).await?;
    }

    Ok(())
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "show" => exec_show(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

async fn exec_show(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let include_token = args.get_flag("token");

    let identity = decode_identity(&config)?;
    println!("You are logged in as {}", identity);

    if include_token {
        // We can `unwrap` because `decode_identity` fetches this too.
        // TODO: maybe decode_identity should take token as a param.
        let token = config.spacetimedb_token().unwrap();
        println!("Your auth token (don't share this!) is {}", token);
    }

    Ok(())
}

async fn spacetimedb_token_cached(config: &mut Config, host: &str, direct_login: bool) -> anyhow::Result<String> {
    // Currently, this token does not expire. However, it will at some point in the future. When that happens,
    // this code will need to happen before any request to a spacetimedb server, rather than at the end of the login flow here.
    if let Some(token) = config.spacetimedb_token() {
        Ok(token.clone())
    } else {
        let token = if direct_login {
            spacetimedb_direct_login(host).await?
        } else {
            let session_id = web_login_cached(config, host).await?;
            spacetimedb_login(host, &session_id).await?
        };
        config.set_spacetimedb_token(token.clone());
        config.save();
        Ok(token)
    }
}

async fn web_login_cached(config: &mut Config, host: &str) -> anyhow::Result<String> {
    if let Some(session_id) = config.web_session_id() {
        // Currently, these session IDs do not expire. At some point in the future, we may also need to check this session ID for validity.
        Ok(session_id.clone())
    } else {
        let session_id = web_login(host).await?;
        config.set_web_session_id(session_id.clone());
        config.save();
        Ok(session_id)
    }
}

#[derive(Deserialize)]
struct WebLoginTokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct WebLoginSessionResponse {
    approved: bool,
    session: Option<String>,
}

#[derive(Deserialize)]
struct WebLoginSessionResponseApproved {
    session_id: String,
}

impl WebLoginSessionResponse {
    fn approved(&self) -> Option<WebLoginSessionResponseApproved> {
        if self.approved {
            Some(WebLoginSessionResponseApproved {
                session_id: self.session.clone().unwrap(),
            })
        } else {
            None
        }
    }
}

async fn web_login(remote: &str) -> Result<String, anyhow::Error> {
    // Users like to provide URLs with trailing slashes, which can cause issues due to double-slashes in the routes below.
    let remote = remote.trim_end_matches('/');

    let route = |path| format!("{}{}", remote, path);

    let client = reqwest::Client::new();

    let response: WebLoginTokenResponse = client
        .get(route("/api/auth/cli/request-login-token"))
        .send()
        .await?
        .json()
        .await?;
    let web_login_request_token = response.token.as_str();

    let browser_url = Url::parse_with_params(route("/login/cli").as_str(), vec![("token", web_login_request_token)])?;
    println!("Opening {} in your browser.", browser_url);
    if webbrowser::open(browser_url.as_str()).is_err() {
        println!("Unable to open your browser! Please open the URL above manually.");
    }

    println!("Waiting to hear response from the server...");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let response: WebLoginSessionResponse = client
            .get(Url::parse_with_params(
                route("/api/auth/cli/status").as_str(),
                vec![("token", web_login_request_token)],
            )?)
            .send()
            .await?
            .json()
            .await?;
        if let Some(approved) = response.approved() {
            println!("Login successful!");
            return Ok(approved.session_id.clone());
        }
    }
}

#[derive(Deserialize)]
struct SpacetimeDBTokenResponse {
    token: String,
}

async fn spacetimedb_login(remote: &str, web_session_id: &String) -> Result<String, anyhow::Error> {
    // Users like to provide URLs with trailing slashes, which can cause issues due to double-slashes in the routes below.
    let remote = remote.trim_end_matches('/');
    let route = |path| format!("{}{}", remote, path);
    let client = reqwest::Client::new();

    let response: SpacetimeDBTokenResponse = client
        .get(route("/api/spacetimedb-token"))
        .header("Authorization", format!("Bearer {}", web_session_id))
        .send()
        .await?
        .json()
        .await?;

    Ok(response.token.clone())
}

#[derive(Debug, Clone, Deserialize)]
struct LocalLoginResponse {
    pub token: String,
}

async fn spacetimedb_direct_login(host: &str) -> Result<String, anyhow::Error> {
    // Users like to provide URLs with trailing slashes, which can cause issues due to double-slashes in the routes below.
    let host = host.trim_end_matches('/');
    let route = |path| format!("{}{}", host, path);
    let client = reqwest::Client::new();

    let response: LocalLoginResponse = client.post(route("/identity")).send().await?.json().await?;

    Ok(response.token)
}