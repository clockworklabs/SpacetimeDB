use crate::util::decode_identity;
use crate::Config;
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use reqwest::Url;
use serde::Deserialize;
use webbrowser;

pub const DEFAULT_AUTH_HOST: &str = "https://spacetimedb.com";

pub fn cli() -> Command {
    Command::new("login")
        .args_conflicts_with_subcommands(true)
        .subcommands(get_subcommands())
        .group(ArgGroup::new("login-method").required(false))
        .arg(
            Arg::new("auth-host")
                .long("auth-host")
                .default_value(DEFAULT_AUTH_HOST)
                .group("login-method")
                .help("Fetch login token from a different host"),
        )
        .arg(
            Arg::new("server")
                .long("server-issued-login")
                .group("login-method")
                .help("Log in to a SpacetimeDB server directly, without going through a global auth server"),
        )
        .arg(
            Arg::new("spacetimedb-token")
                .long("token")
                .group("login-method")
                .help("Bypass the login flow and use a login token directly"),
        )
        .about("Manage your login to the SpacetimeDB CLI")
}

fn get_subcommands() -> Vec<Command> {
    vec![Command::new("show")
        .arg(
            Arg::new("token")
                .long("token")
                .action(ArgAction::SetTrue)
                .help("Also show the auth token"),
        )
        .about("Show the current login info")]
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    if let Some((cmd, subcommand_args)) = args.subcommand() {
        return exec_subcommand(config, cmd, subcommand_args).await;
    }

    let spacetimedb_token: Option<&String> = args.get_one("spacetimedb-token");
    let host: &String = args.get_one("auth-host").unwrap();
    let host = Url::parse(host)?;
    let server_issued_login: Option<&String> = args.get_one("server");

    if let Some(token) = spacetimedb_token {
        config.set_spacetimedb_token(token.clone());
        config.save();
        return Ok(());
    }

    if let Some(server) = server_issued_login {
        let host = Url::parse(&config.get_host_url(Some(server))?)?;
        spacetimedb_token_cached(&mut config, &host, true).await?;
    } else {
        spacetimedb_token_cached(&mut config, &host, false).await?;
    }

    Ok(())
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "show" => exec_show(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

async fn exec_show(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let include_token = args.get_flag("token");

    let token = if let Some(token) = config.spacetimedb_token() {
        token
    } else {
        println!("You are not logged in. Run `spacetime login` to log in.");
        return Ok(());
    };

    let identity = decode_identity(token)?;
    println!("You are logged in as {identity}");

    if include_token {
        println!("Your auth token (don't share this!) is {token}");
    }

    Ok(())
}

async fn spacetimedb_token_cached(config: &mut Config, host: &Url, direct_login: bool) -> anyhow::Result<String> {
    // Currently, this token does not expire. However, it will at some point in the future. When that happens,
    // this code will need to happen before any request to a spacetimedb server, rather than at the end of the login flow here.
    if let Some(token) = config.spacetimedb_token() {
        println!("You are already logged in.");
        println!("If you want to log out, use spacetime logout.");
        Ok(token.clone())
    } else {
        spacetimedb_login_force(config, host, direct_login).await
    }
}

pub async fn spacetimedb_login_force(config: &mut Config, host: &Url, direct_login: bool) -> anyhow::Result<String> {
    let token = if direct_login {
        let token = spacetimedb_direct_login(host).await?;
        println!("We have logged in directly to your target server.");
        println!("WARNING: This login will NOT work for any other servers.");
        token
    } else {
        let session_token = web_login_cached(config, host).await?;
        spacetimedb_login(host, &session_token).await?
    };
    config.set_spacetimedb_token(token.clone());
    config.save();

    Ok(token)
}

async fn web_login_cached(config: &mut Config, host: &Url) -> anyhow::Result<String> {
    if let Some(session_token) = config.web_session_token() {
        // Currently, these session tokens do not expire. At some point in the future, we may also need to check this session token for validity.
        Ok(session_token.clone())
    } else {
        let session_token = web_login(host).await?;
        config.set_web_session_token(session_token.clone());
        config.save();
        Ok(session_token)
    }
}

#[derive(Clone, Deserialize)]
struct WebLoginTokenData {
    token: String,
}

#[derive(Clone, Deserialize)]
struct WebLoginTokenResponse {
    success: bool,
    data: WebLoginTokenData,
}

#[derive(Clone, Deserialize)]
struct WebLoginSessionResponse {
    success: bool,
    error: Option<String>,
    data: Option<WebLoginSessionData>,
}

#[derive(Clone, Deserialize)]
struct WebLoginSessionData {
    approved: bool,

    #[serde(rename = "sessionToken")]
    session_token: Option<String>,
}

#[derive(Clone, Deserialize)]
struct WebLoginSessionResponseApproved {
    session_token: String,
}

impl WebLoginSessionResponse {
    fn approved(self) -> anyhow::Result<Option<WebLoginSessionResponseApproved>> {
        if !self.success {
            return Err(anyhow::anyhow!(self
                .error
                .clone()
                .unwrap_or("Unknown error".to_string())));
        }

        let data = self.data.ok_or(anyhow::anyhow!("Response data is missing."))?;
        if !data.approved {
            // Approved is false, no session token expected
            return Ok(None);
        }

        let session_token = data
            .session_token
            .ok_or(anyhow::anyhow!("Session token is missing in response.".to_string()))?;
        Ok(Some(WebLoginSessionResponseApproved {
            session_token: session_token.clone(),
        }))
    }
}

async fn web_login(remote: &Url) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();

    let response: WebLoginTokenResponse = client
        .post(remote.join("/api/auth/cli/login/request-token")?)
        .send()
        .await?
        .json()
        .await?;

    if !response.success {
        return Err(anyhow::anyhow!("Failed to request token"));
    }

    let web_login_request_token = response.data.token.as_str();

    let mut browser_url = remote.join("login/cli")?;
    browser_url
        .query_pairs_mut()
        .append_pair("token", web_login_request_token);
    println!("Opening {browser_url} in your browser.");
    if webbrowser::open(browser_url.as_str()).is_err() {
        println!("Unable to open your browser! Please open the URL above manually.");
    }

    println!("Waiting to hear response from the server...");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut status_url = remote.join("api/auth/cli/status")?;
        status_url
            .query_pairs_mut()
            .append_pair("token", web_login_request_token);
        let response: WebLoginSessionResponse = client.get(status_url).send().await?.json().await?;
        if let Some(approved) = response.approved()? {
            println!("Login successful!");
            return Ok(approved.session_token.clone());
        }
    }
}

#[derive(Deserialize, Debug)]
struct SpacetimeDBTokenResponse {
    success: bool,
    error: Option<String>,
    data: Option<SpacetimeDBTokenData>,
}

#[derive(Deserialize, Debug)]
struct SpacetimeDBTokenData {
    token: String,
}

async fn spacetimedb_login(remote: &Url, web_session_token: &String) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();

    let response: SpacetimeDBTokenResponse = client
        .post(remote.join("api/spacetimedb-token")?)
        .header("Authorization", format!("Bearer {web_session_token}"))
        .send()
        .await?
        .json()
        .await?;

    if !response.success {
        return Err(anyhow::anyhow!(
            "Failed to get token: {}",
            response.error.unwrap_or("Unknown error".to_string())
        ));
    }
    Ok(response.data.unwrap().token.clone())
}

#[derive(Debug, Clone, Deserialize)]
struct LocalLoginResponse {
    pub token: String,
}

async fn spacetimedb_direct_login(host: &Url) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();
    let response: LocalLoginResponse = client
        .post(host.join("/v1/identity")?)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response.token)
}
