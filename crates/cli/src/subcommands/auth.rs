use crate::login::DEFAULT_AUTH_HOST;
use crate::Config;
use clap::{Arg, ArgMatches, Command, ValueEnum};
use reqwest::Client;

#[derive(Clone, Debug, ValueEnum)]
pub enum IdentityProvider {
    Google,
    Twitch,
    Discord,
    Kick,
    Github,
    Trackmania,
}

impl std::fmt::Display for IdentityProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_possible_value().unwrap().get_name())
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ClientSetting {
    Name,
    Private,
    Web,
    Native,
    #[value(name = "redirect_uris")]
    RedirectUris,
    #[value(name = "post_logout_redirect_uris")]
    PostLogoutRedirectUris,
}

impl std::fmt::Display for ClientSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_possible_value().unwrap().get_name())
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum AuthConfigSetting {
    #[value(name = "display_name")]
    DisplayName,
    #[value(name = "favicon_url")]
    FaviconUrl,
    #[value(name = "color.text")]
    ColorText,
    #[value(name = "color.background")]
    ColorBackground,
    #[value(name = "color.primary")]
    ColorPrimary,
    #[value(name = "color.input")]
    ColorInput,
    #[value(name = "color.border")]
    ColorBorder,
    #[value(name = "login.email")]
    LoginEmail,
    #[value(name = "login.anonymous")]
    LoginAnonymous,
    #[value(name = "steam.publisher_key")]
    SteamPublisherKey,
    #[value(name = "steam.app_ids")]
    SteamAppIds,
}

impl std::fmt::Display for AuthConfigSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_possible_value().unwrap().get_name())
    }
}

const DEFAULT_CLIENT_NAME: &str = "Default Client";

/// Overrides the SpacetimeAuth management endpoint, e.g. to target a local dev server.
const SPACETIMEAUTH_API_ENV: &str = "SPACETIMEAUTH_API";

fn api_url() -> String {
    std::env::var(SPACETIMEAUTH_API_ENV).unwrap_or_else(|_| format!("{DEFAULT_AUTH_HOST}/api/spacetimeauth/cli"))
}

/// Sends `body` to the SpacetimeAuth management API and prints the response.
async fn send(config: &Config, body: serde_json::Value) -> anyhow::Result<()> {
    let token = config
        .web_session_token()
        .ok_or_else(|| anyhow::anyhow!("You are not logged in. Run `spacetime login` first."))?;

    let response = Client::builder()
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .build()?
        .post(api_url())
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("{status}: {text}");
    }

    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(json) => println!("{}", serde_json::to_string_pretty(&json)?),
        Err(_) => println!("{text}"),
    }

    Ok(())
}

fn database_arg() -> Arg {
    Arg::new("database").required(true).help("The name of the database")
}

fn idp_arg() -> Arg {
    Arg::new("idp")
        .required(true)
        .value_parser(clap::builder::EnumValueParser::<IdentityProvider>::new())
        .help("The identity provider to configure")
}

fn client_name_arg() -> Arg {
    Arg::new("name")
        .long("name")
        .default_value(DEFAULT_CLIENT_NAME)
        .help("The client name")
}

pub fn cli() -> Command {
    Command::new("auth")
        .about("Manage SpacetimeAuth for a database")
        .subcommand_required(true)
        .subcommands(get_subcommands())
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("config")
            .about("Manage SpacetimeAuth configuration for a database")
            .subcommand_required(true)
            .subcommand(
                Command::new("set")
                    .about("Set a SpacetimeAuth configuration value for a database")
                    .arg(database_arg())
                    .arg(
                        Arg::new("key")
                            .required(true)
                            .value_parser(clap::builder::EnumValueParser::<AuthConfigSetting>::new())
                            .help("The setting to configure"),
                    )
                    .arg(
                        Arg::new("value")
                            .required(true)
                            .help("The value to assign to the setting"),
                    ),
            )
            .subcommand(
                Command::new("reset")
                    .about("Reset all SpacetimeAuth configuration for a database")
                    .arg(database_arg()),
            ),
        Command::new("idp")
            .about("Manage identity providers for a database")
            .subcommand_required(true)
            .subcommand(
                Command::new("set")
                    .about("Configure an identity provider for a database")
                    .arg(database_arg())
                    .arg(idp_arg())
                    .arg(Arg::new("client_id").required(true).help("The OAuth client ID"))
                    .arg(Arg::new("client_secret").required(true).help("The OAuth client secret")),
            )
            .subcommand(
                Command::new("enable")
                    .about("Enable an identity provider for a database")
                    .arg(database_arg())
                    .arg(idp_arg()),
            )
            .subcommand(
                Command::new("disable")
                    .about("Disable an identity provider for a database")
                    .arg(database_arg())
                    .arg(idp_arg()),
            ),
        Command::new("client")
            .about("Manage OAuth clients for a database")
            .subcommand_required(true)
            .subcommand(
                Command::new("create")
                    .about("Create a new OAuth client")
                    .arg(database_arg())
                    .arg(client_name_arg())
                    .arg(
                        Arg::new("private")
                            .long("private")
                            .action(clap::ArgAction::SetTrue)
                            .help("Create the client as private (requires a client secret for token exchange)"),
                    ),
            )
            .subcommand(
                Command::new("delete")
                    .about("Delete an OAuth client")
                    .arg(database_arg())
                    .arg(client_name_arg()),
            )
            .subcommand(
                Command::new("get")
                    .about("Get an OAuth client")
                    .arg(database_arg())
                    .arg(client_name_arg())
                    .arg(
                        Arg::new("include-secret")
                            .long("include-secret")
                            .action(clap::ArgAction::SetTrue)
                            .help("Include the client secret in the output"),
                    ),
            )
            .subcommand(
                Command::new("set")
                    .about("Set a configuration value for an OAuth client")
                    .arg(database_arg())
                    .arg(
                        Arg::new("key")
                            .required(true)
                            .value_parser(clap::builder::EnumValueParser::<ClientSetting>::new())
                            .help("The setting to configure"),
                    )
                    .arg(
                        Arg::new("value")
                            .required(true)
                            .help("The value to assign to the setting"),
                    )
                    .arg(client_name_arg()),
            ),
    ]
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, args) = args.subcommand().expect("Subcommand required");
    let (subcmd, args) = args.subcommand().expect("Subcommand required");
    let database = args.get_one::<String>("database").unwrap();

    let body = match (cmd, subcmd) {
        ("config", "set") => {
            let key = args.get_one::<AuthConfigSetting>("key").unwrap();
            let value = args.get_one::<String>("value").unwrap();
            validate_config_setting(key, value)?;
            serde_json::json!({
                "action": "config.set",
                "database": database,
                "key": key.to_string(),
                "value": value,
            })
        }
        ("config", "reset") => serde_json::json!({
            "action": "config.reset",
            "database": database,
        }),
        ("idp", "set") => serde_json::json!({
            "action": "idp.set",
            "database": database,
            "idp": args.get_one::<IdentityProvider>("idp").unwrap().to_string(),
            "client_id": args.get_one::<String>("client_id").unwrap(),
            "client_secret": args.get_one::<String>("client_secret").unwrap(),
        }),
        ("idp", toggle @ ("enable" | "disable")) => serde_json::json!({
            "action": "idp.toggle",
            "database": database,
            "idp": args.get_one::<IdentityProvider>("idp").unwrap().to_string(),
            "enabled": toggle == "enable",
        }),
        ("client", "create") => serde_json::json!({
            "action": "client.create",
            "database": database,
            "name": args.get_one::<String>("name").unwrap(),
            "private": args.get_flag("private"),
        }),
        ("client", "delete") => serde_json::json!({
            "action": "client.delete",
            "database": database,
            "name": args.get_one::<String>("name").unwrap(),
        }),
        ("client", "get") => serde_json::json!({
            "action": "client.get",
            "database": database,
            "name": args.get_one::<String>("name").unwrap(),
            "include_secret": args.get_flag("include-secret"),
        }),
        ("client", "set") => {
            let key = args.get_one::<ClientSetting>("key").unwrap();
            let value = args.get_one::<String>("value").unwrap();
            validate_client_setting(key, value)?;
            serde_json::json!({
                "action": "client.set",
                "database": database,
                "name": args.get_one::<String>("name").unwrap(),
                "key": key.to_string(),
                "value": value,
            })
        }
        (cmd, subcmd) => anyhow::bail!("Invalid subcommand: {cmd} {subcmd}"),
    };

    send(&config, body).await
}

fn validate_bool(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0"),
        "expected a boolean (true/false), got: {value:?}"
    );
    Ok(())
}

/// Validates each entry of a comma-separated list. An empty value clears the list.
fn validate_list(value: &str, validate_entry: impl Fn(&str) -> anyhow::Result<()>) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        return Ok(());
    }
    for entry in value.split(',') {
        let entry = entry.trim();
        anyhow::ensure!(!entry.is_empty(), "list must not contain empty entries: {value:?}");
        validate_entry(entry)?;
    }
    Ok(())
}

fn validate_client_setting(key: &ClientSetting, value: &str) -> anyhow::Result<()> {
    match key {
        ClientSetting::Name => {
            anyhow::ensure!(!value.trim().is_empty(), "client name cannot be empty");
        }
        ClientSetting::Private | ClientSetting::Web | ClientSetting::Native => validate_bool(value)?,
        ClientSetting::RedirectUris | ClientSetting::PostLogoutRedirectUris => validate_list(value, |uri| {
            url::Url::parse(uri).map_err(|e| anyhow::anyhow!("invalid URI {uri:?}: {e}"))?;
            Ok(())
        })?,
    }
    Ok(())
}

fn validate_config_setting(key: &AuthConfigSetting, value: &str) -> anyhow::Result<()> {
    match key {
        AuthConfigSetting::LoginEmail | AuthConfigSetting::LoginAnonymous => validate_bool(value)?,
        AuthConfigSetting::SteamAppIds => validate_list(value, |id| {
            id.parse::<u32>()
                .map_err(|_| anyhow::anyhow!("invalid Steam app ID: {id:?}, expected a positive integer"))?;
            Ok(())
        })?,
        // Validated by the server.
        AuthConfigSetting::DisplayName
        | AuthConfigSetting::FaviconUrl
        | AuthConfigSetting::ColorText
        | AuthConfigSetting::ColorBackground
        | AuthConfigSetting::ColorPrimary
        | AuthConfigSetting::ColorInput
        | AuthConfigSetting::ColorBorder
        | AuthConfigSetting::SteamPublisherKey => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_settings() {
        for value in ["true", "FALSE", "1", "0"] {
            assert!(validate_config_setting(&AuthConfigSetting::LoginEmail, value).is_ok());
            assert!(validate_client_setting(&ClientSetting::Private, value).is_ok());
        }
        for value in ["yes", "", "2"] {
            assert!(validate_config_setting(&AuthConfigSetting::LoginAnonymous, value).is_err());
            assert!(validate_client_setting(&ClientSetting::Web, value).is_err());
        }
    }

    #[test]
    fn steam_app_ids() {
        assert!(validate_config_setting(&AuthConfigSetting::SteamAppIds, "480, 730").is_ok());
        assert!(validate_config_setting(&AuthConfigSetting::SteamAppIds, "480,abc").is_err());
        assert!(validate_config_setting(&AuthConfigSetting::SteamAppIds, "480,,730").is_err());
        assert!(validate_config_setting(&AuthConfigSetting::SteamAppIds, "-1").is_err());
    }

    #[test]
    fn redirect_uris() {
        let key = ClientSetting::RedirectUris;
        assert!(validate_client_setting(&key, "https://example.com/cb, myapp://callback").is_ok());
        assert!(validate_client_setting(&key, "not a uri").is_err());
        assert!(validate_client_setting(&key, "https://example.com/cb,").is_err());
    }

    #[test]
    fn empty_list_clears() {
        assert!(validate_config_setting(&AuthConfigSetting::SteamAppIds, "").is_ok());
        assert!(validate_client_setting(&ClientSetting::RedirectUris, " ").is_ok());
        assert!(validate_client_setting(&ClientSetting::PostLogoutRedirectUris, "").is_ok());
    }

    #[test]
    fn client_name_not_empty() {
        assert!(validate_client_setting(&ClientSetting::Name, "My Client").is_ok());
        assert!(validate_client_setting(&ClientSetting::Name, "  ").is_err());
    }

    #[test]
    fn cli_parses() {
        cli().debug_assert();
        let matches = cli()
            .try_get_matches_from([
                "auth",
                "client",
                "set",
                "my-db",
                "redirect_uris",
                "https://a.b",
                "--name",
                "c",
            ])
            .unwrap();
        let (_, client) = matches.subcommand().unwrap();
        let (_, set) = client.subcommand().unwrap();
        assert_eq!(set.get_one::<String>("database").unwrap(), "my-db");
        assert_eq!(set.get_one::<String>("name").unwrap(), "c");
        assert!(matches!(
            set.get_one::<ClientSetting>("key").unwrap(),
            ClientSetting::RedirectUris
        ));
    }
}
