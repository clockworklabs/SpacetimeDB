use crate::Config;
use clap::{Arg, ArgMatches, Command, ValueEnum};
use reqwest::Client;
use spacetimedb_paths::SpacetimePaths;

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

const DEFAULT_CLIENT_NAME: &str = "Default Client";

const SPACETIMEAUTH_API: &str = "https://spacetimedb.com/api/spacetimeauth/cli/";

async fn request<T>(config: Config, body: &T) -> anyhow::Result<reqwest::Response>
where
    T: serde::Serialize,
{
    let url = std::env::var("SPACETIMEAUTH_API").unwrap_or_else(|_| SPACETIMEAUTH_API.to_string());
    let response = Client::builder()
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap()
        .request(reqwest::Method::POST, url)
        .bearer_auth(config.web_session_token().expect("SpacetimeDB token required"))
        .json(body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("{status}: {text}");
    }

    Ok(response)
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

pub fn cli() -> Command {
    Command::new("auth")
        .about("Manage SpacetimeAuth for a database")
        .args_conflicts_with_subcommands(true)
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
                    .arg(
                        Arg::new("database")
                            .required(true)
                            .help("The name or identity of the database"),
                    )
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
                    .arg(
                        Arg::new("database")
                            .required(true)
                            .help("The name or identity of the database"),
                    ),
            ),
        Command::new("idp")
            .about("Manage identity providers for a database")
            .subcommand_required(true)
            .subcommand(
                Command::new("set")
                    .about("Configure an identity provider for a database")
                    .arg(
                        Arg::new("database")
                            .required(true)
                            .help("The name or identity of the database"),
                    )
                    .arg(
                        Arg::new("idp")
                            .required(true)
                            .value_parser(clap::builder::EnumValueParser::<IdentityProvider>::new())
                            .help("The identity provider to configure"),
                    )
                    .arg(Arg::new("client_id").required(true).help("The OAuth client ID"))
                    .arg(Arg::new("client_secret").required(true).help("The OAuth client secret")),
            )
            .subcommand(idp_toggle_command(
                "enable",
                "Enable an identity provider for a database",
            ))
            .subcommand(idp_toggle_command(
                "disable",
                "Disable an identity provider for a database",
            )),
        Command::new("client")
            .about("Manage OAuth clients for SpacetimeAuth")
            .subcommand_required(true)
            .subcommand(
                Command::new("create")
                    .about("Create a new OAuth client")
                    .arg(
                        Arg::new("name")
                            .required(false)
                            .default_value(DEFAULT_CLIENT_NAME)
                            .help("The client name"),
                    )
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
                    .arg(
                        Arg::new("name")
                            .required(false)
                            .default_value(DEFAULT_CLIENT_NAME)
                            .help("The client name"),
                    ),
            )
            .subcommand(
                Command::new("get")
                    .about("Get an OAuth client")
                    .arg(
                        Arg::new("name")
                            .required(false)
                            .default_value(DEFAULT_CLIENT_NAME)
                            .help("The client name"),
                    )
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
                    .after_help(
                        "ARGS:\n  [name]  Client name (default: \"Default Client\")\n  <key>   \
                         Setting to update: name, private, web, native, redirect_uris, \
                         post_logout_redirect_uris\n  <value> Value to assign",
                    )
                    .arg(
                        Arg::new("args")
                            .num_args(2..=3)
                            .required(true)
                            .value_names(["key", "value"])
                            .help("[name] <key> <value>"),
                    ),
            ),
    ]
}

fn idp_toggle_command(name: &'static str, about: &'static str) -> Command {
    Command::new(name)
        .about(about)
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database"),
        )
        .arg(
            Arg::new("idp")
                .required(true)
                .value_parser(clap::builder::EnumValueParser::<IdentityProvider>::new())
                .help("The identity provider to configure"),
        )
}


pub async fn exec(config: Config, paths: &SpacetimePaths, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, paths, cmd, subcommand_args).await
}

async fn exec_subcommand(
    config: Config,
    _paths: &SpacetimePaths,
    cmd: &str,
    args: &ArgMatches,
) -> Result<(), anyhow::Error> {
    match cmd {
        "config" => exec_config(config, args).await,
        "idp" => exec_idp(config, args).await,
        "client" => exec_client(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

async fn exec_client(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    match cmd {
        "create" => exec_client_create(config, subcommand_args).await,
        "delete" => exec_client_delete(config, subcommand_args).await,
        "get" => exec_client_get(config, subcommand_args).await,
        "set" => exec_client_set(config, subcommand_args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

async fn exec_client_create(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.get_one::<String>("name").unwrap();
    let private = args.get_flag("private");

    let response = request(
        config,
        &serde_json::json!({
            "action": "client.create",
            "name": name,
            "private": private,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

async fn exec_client_delete(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.get_one::<String>("name").unwrap();

    let response = request(
        config,
        &serde_json::json!({
            "action": "client.delete",
            "name": name,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

async fn exec_client_get(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.get_one::<String>("name").unwrap();
    let include_secret = args.get_flag("include-secret");

    let response = request(
        config,
        &serde_json::json!({
            "action": "client.get",
            "name": name,
            "include_secret": include_secret,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

async fn exec_client_set(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let raw: Vec<&String> = args.get_many::<String>("args").unwrap().collect();

    // Disambiguate [name] <key> <value>: with 2 args the name is omitted.
    let (client_name, key_str, value_str) = match raw.as_slice() {
        [key, value] => (DEFAULT_CLIENT_NAME, key.as_str(), value.as_str()),
        [name, key, value] => (name.as_str(), key.as_str(), value.as_str()),
        _ => unreachable!(),
    };

    let key = ClientSetting::from_str(key_str, true).map_err(|_| {
        anyhow::anyhow!(
            "invalid key: {key_str:?}. Valid keys: name, private, web, native, \
             redirect_uris, post_logout_redirect_uris"
        )
    })?;

    validate_client_setting(&key, value_str)?;

    let response = request(
        config,
        &serde_json::json!({
            "action": "client.set",
            "name": client_name,
            "key": key.to_string(),
            "value": value_str,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

fn validate_client_setting(key: &ClientSetting, value: &str) -> Result<(), anyhow::Error> {
    match key {
        ClientSetting::Name => {
            anyhow::ensure!(!value.trim().is_empty(), "client name cannot be empty");
        }
        ClientSetting::Private | ClientSetting::Web | ClientSetting::Native => {
            anyhow::ensure!(
                matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0"),
                "expected a boolean (true/false), got: {value:?}"
            );
        }
        ClientSetting::RedirectUris | ClientSetting::PostLogoutRedirectUris => {
            for uri in value.split(',') {
                let uri = uri.trim();
                anyhow::ensure!(!uri.is_empty(), "URI list must not contain empty entries");
                url::Url::parse(uri).map_err(|e| anyhow::anyhow!("invalid URI {uri:?}: {e}"))?;
            }
        }
    }
    Ok(())
}

async fn exec_config(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    match cmd {
        "set" => exec_config_set(config, subcommand_args).await,
        "reset" => exec_config_reset(config, subcommand_args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

async fn exec_config_set(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let key = args.get_one::<AuthConfigSetting>("key").unwrap();
    let value = args.get_one::<String>("value").unwrap();

    validate_value(key, value)?;

    let response = request(
        config,
        &serde_json::json!({
            "action": "config.set",
            "database": database,
            "key": key.to_string(),
            "value": value,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

async fn exec_config_reset(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();

    let response = request(
        config,
        &serde_json::json!({
            "action": "config.reset",
            "database": database,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

async fn exec_idp(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    match cmd {
        "set" => exec_idp_set(config, subcommand_args).await,
        "enable" => exec_idp_toggle(config, subcommand_args, true).await,
        "disable" => exec_idp_toggle(config, subcommand_args, false).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

async fn exec_idp_set(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let idp = args.get_one::<IdentityProvider>("idp").unwrap();
    let client_id = args.get_one::<String>("client_id").unwrap();
    let client_secret = args.get_one::<String>("client_secret").unwrap();

    let response = request(
        config,
        &serde_json::json!({
            "action": "idp.set",
            "database": database,
            "idp": idp.to_string(),
            "client_id": client_id,
            "client_secret": client_secret,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

async fn exec_idp_toggle(config: Config, args: &ArgMatches, enabled: bool) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let idp = args.get_one::<IdentityProvider>("idp").unwrap();

    let response = request(
        config,
        &serde_json::json!({
            "action": "idp.toggle",
            "database": database,
            "idp": idp.to_string(),
            "enabled": enabled,
        }),
    )
    .await?;

    let response_text = response.text().await?;
    println!("{response_text}");

    Ok(())
}

fn validate_value(setting: &AuthConfigSetting, value: &str) -> Result<(), anyhow::Error> {
    match setting {
        AuthConfigSetting::ColorText
        | AuthConfigSetting::ColorBackground
        | AuthConfigSetting::ColorPrimary
        | AuthConfigSetting::ColorInput
        | AuthConfigSetting::ColorBorder => {
            anyhow::ensure!(is_valid_css_color(value), "invalid CSS color: {value:?}");
        }
        AuthConfigSetting::FaviconUrl => {
            url::Url::parse(value).map_err(|e| anyhow::anyhow!("invalid URL: {e}"))?;
        }
        AuthConfigSetting::LoginEmail | AuthConfigSetting::LoginAnonymous => {
            anyhow::ensure!(
                matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0"),
                "expected a boolean (true/false), got: {value:?}"
            );
        }
        AuthConfigSetting::SteamAppIds => {
            for part in value.split(',') {
                let part = part.trim();
                anyhow::ensure!(!part.is_empty(), "Steam app ID list must not contain empty entries");
                part.parse::<u32>()
                    .map_err(|_| anyhow::anyhow!("invalid Steam app ID: {part:?}, expected a positive integer"))?;
            }
        }
        AuthConfigSetting::DisplayName | AuthConfigSetting::SteamPublisherKey => {}
    }
    Ok(())
}

fn is_valid_css_color(value: &str) -> bool {
    const NAMED: &[&str] = &[
        "aliceblue",
        "antiquewhite",
        "aqua",
        "aquamarine",
        "azure",
        "beige",
        "bisque",
        "black",
        "blanchedalmond",
        "blue",
        "blueviolet",
        "brown",
        "burlywood",
        "cadetblue",
        "chartreuse",
        "chocolate",
        "coral",
        "cornflowerblue",
        "cornsilk",
        "crimson",
        "cyan",
        "darkblue",
        "darkcyan",
        "darkgoldenrod",
        "darkgray",
        "darkgreen",
        "darkgrey",
        "darkkhaki",
        "darkmagenta",
        "darkolivegreen",
        "darkorange",
        "darkorchid",
        "darkred",
        "darksalmon",
        "darkseagreen",
        "darkslateblue",
        "darkslategray",
        "darkslategrey",
        "darkturquoise",
        "darkviolet",
        "deeppink",
        "deepskyblue",
        "dimgray",
        "dimgrey",
        "dodgerblue",
        "firebrick",
        "floralwhite",
        "forestgreen",
        "fuchsia",
        "gainsboro",
        "ghostwhite",
        "gold",
        "goldenrod",
        "gray",
        "green",
        "greenyellow",
        "grey",
        "honeydew",
        "hotpink",
        "indianred",
        "indigo",
        "ivory",
        "khaki",
        "lavender",
        "lavenderblush",
        "lawngreen",
        "lemonchiffon",
        "lightblue",
        "lightcoral",
        "lightcyan",
        "lightgoldenrodyellow",
        "lightgray",
        "lightgreen",
        "lightgrey",
        "lightpink",
        "lightsalmon",
        "lightseagreen",
        "lightskyblue",
        "lightslategray",
        "lightslategrey",
        "lightsteelblue",
        "lightyellow",
        "lime",
        "limegreen",
        "linen",
        "magenta",
        "maroon",
        "mediumaquamarine",
        "mediumblue",
        "mediumorchid",
        "mediumpurple",
        "mediumseagreen",
        "mediumslateblue",
        "mediumspringgreen",
        "mediumturquoise",
        "mediumvioletred",
        "midnightblue",
        "mintcream",
        "mistyrose",
        "moccasin",
        "navajowhite",
        "navy",
        "oldlace",
        "olive",
        "olivedrab",
        "orange",
        "orangered",
        "orchid",
        "palegoldenrod",
        "palegreen",
        "paleturquoise",
        "palevioletred",
        "papayawhip",
        "peachpuff",
        "peru",
        "pink",
        "plum",
        "powderblue",
        "purple",
        "rebeccapurple",
        "red",
        "rosybrown",
        "royalblue",
        "saddlebrown",
        "salmon",
        "sandybrown",
        "seagreen",
        "seashell",
        "sienna",
        "silver",
        "skyblue",
        "slateblue",
        "slategray",
        "slategrey",
        "snow",
        "springgreen",
        "steelblue",
        "tan",
        "teal",
        "thistle",
        "tomato",
        "transparent",
        "turquoise",
        "violet",
        "wheat",
        "white",
        "whitesmoke",
        "yellow",
        "yellowgreen",
    ];

    let lower = value.to_lowercase();

    if NAMED.contains(&lower.as_str()) {
        return true;
    }

    if let Some(hex) = lower.strip_prefix('#') {
        let n = hex.len();
        return (n == 3 || n == 4 || n == 6 || n == 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }

    // rgb(), rgba(), hsl(), hsla()
    let func_re =
        regex::Regex::new(r"(?i)^(rgba?|hsla?)\(\s*[\d.]+%?\s*,\s*[\d.]+%?\s*,\s*[\d.]+%?\s*(?:,\s*[\d.]+\s*)?\)$")
            .unwrap();
    func_re.is_match(value)
}
