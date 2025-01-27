use anyhow::Context;
use base64::{
    engine::general_purpose::STANDARD as BASE_64_STD, engine::general_purpose::STANDARD_NO_PAD as BASE_64_STD_NO_PAD,
    Engine as _,
};
use reqwest::{RequestBuilder, Url};
use spacetimedb::auth::identity::{IncomingClaims, SpacetimeIdentityClaims};
use spacetimedb_client_api_messages::name::{DnsLookupResponse, RegisterTldResult, ReverseDNSResponse};
use spacetimedb_lib::Identity;
use std::io::Write;
use std::path::Path;

use crate::config::Config;
use crate::login::{spacetimedb_login_force, DEFAULT_AUTH_HOST};

pub const UNSTABLE_WARNING: &str = "WARNING: This command is UNSTABLE and subject to breaking changes.";

/// Determine the identity of the `database`.
pub async fn database_identity(
    config: &Config,
    name_or_identity: &str,
    server: Option<&str>,
) -> Result<Identity, anyhow::Error> {
    if let Ok(identity) = Identity::from_hex(name_or_identity) {
        return Ok(identity);
    }
    match spacetime_dns(config, name_or_identity, server).await? {
        DnsLookupResponse::Success { domain: _, identity } => Ok(identity),
        DnsLookupResponse::Failure { domain } => Err(anyhow::anyhow!("The dns resolution of `{}` failed.", domain)),
    }
}

/// Converts a name to a database identity.
pub async fn spacetime_dns(
    config: &Config,
    domain: &str,
    server: Option<&str>,
) -> Result<DnsLookupResponse, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("{}/database/dns/{}", config.get_host_url(server)?, domain);
    let res = client.get(url).send().await?.error_for_status()?;
    let bytes = res.bytes().await.unwrap();
    Ok(serde_json::from_slice(&bytes[..]).unwrap())
}

/// Registers the given top level domain to the given identity. If None is passed in as identity, the default
/// identity will be looked up in the config and it will be used instead. Returns Ok() if the
/// domain is successfully registered, returns Err otherwise.
pub async fn spacetime_register_tld(
    config: &mut Config,
    tld: &str,
    server: Option<&str>,
    interactive: bool,
) -> Result<RegisterTldResult, anyhow::Error> {
    let auth_header = get_auth_header(config, false, server, interactive).await?;

    // TODO(jdetter): Fix URL encoding on specifying this domain
    let builder = reqwest::Client::new()
        .get(format!("{}/database/register_tld?tld={}", config.get_host_url(server)?, tld).as_str());
    let builder = add_auth_header_opt(builder, &auth_header);

    let res = builder.send().await?.error_for_status()?;
    let bytes = res.bytes().await.unwrap();
    Ok(serde_json::from_slice(&bytes[..]).unwrap())
}

pub async fn spacetime_server_fingerprint(url: &str) -> anyhow::Result<String> {
    let builder = reqwest::Client::new().get(format!("{}/identity/public-key", url).as_str());
    let res = builder.send().await?.error_for_status()?;
    let fingerprint = res.text().await?;
    Ok(fingerprint)
}

/// Returns all known names for the given identity.
pub async fn spacetime_reverse_dns(
    config: &Config,
    identity: &str,
    server: Option<&str>,
) -> Result<ReverseDNSResponse, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("{}/database/reverse_dns/{}", config.get_host_url(server)?, identity);
    let res = client.get(url).send().await?.error_for_status()?;
    let bytes = res.bytes().await.unwrap();
    Ok(serde_json::from_slice(&bytes[..]).unwrap())
}

/// Add an authorization header, if provided, to the request `builder`.
pub fn add_auth_header_opt(mut builder: RequestBuilder, auth_header: &Option<String>) -> RequestBuilder {
    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }
    builder
}

/// Gets the `auth_header` for a request to the server depending on how you want
/// to identify yourself.  If you specify `anon_identity = true` then no
/// `auth_header` is returned. If you specify an identity this function will try
/// to find the identity in the config file. If no identity can be found, the
/// program will `exit(1)`. If you do not specify an identity this function will
/// either get the default identity if one exists or create and save a new
/// default identity returning the one that was just created.
///
/// # Arguments
///  * `config` - The config file reference
///  * `anon_identity` - Whether or not to just use an anonymous identity (no identity)
///  * `identity_or_name` - The identity to try to lookup, which is typically provided from the command line
pub async fn get_auth_header(
    config: &mut Config,
    anon_identity: bool,
    target_server: Option<&str>,
    interactive: bool,
) -> anyhow::Result<Option<String>> {
    if anon_identity {
        Ok(None)
    } else {
        let token = get_login_token_or_log_in(config, target_server, interactive).await?;
        // The current form is: Authorization: Basic base64("token:<token>")
        let mut auth_header = String::new();
        auth_header.push_str(format!("Basic {}", BASE_64_STD.encode(format!("token:{}", token))).as_str());
        Ok(Some(auth_header))
    }
}

pub const VALID_PROTOCOLS: [&str; 2] = ["http", "https"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ModuleLanguage {
    Csharp,
    Rust,
}
impl clap::ValueEnum for ModuleLanguage {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Csharp, Self::Rust]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Csharp => Some(clap::builder::PossibleValue::new("csharp").aliases(["c#", "cs", "C#", "CSharp"])),
            Self::Rust => Some(clap::builder::PossibleValue::new("rust").aliases(["rs", "Rust"])),
        }
    }
}

pub fn detect_module_language(path_to_project: &Path) -> anyhow::Result<ModuleLanguage> {
    // TODO: Possible add a config file durlng spacetime init with the language
    // check for Cargo.toml
    if path_to_project.join("Cargo.toml").exists() {
        Ok(ModuleLanguage::Rust)
    } else if path_to_project
        .read_dir()
        .unwrap()
        .any(|entry| entry.unwrap().path().extension() == Some("csproj".as_ref()))
    {
        Ok(ModuleLanguage::Csharp)
    } else {
        anyhow::bail!("Could not detect the language of the module. Are you in a SpacetimeDB project directory?")
    }
}

pub fn url_to_host_and_protocol(url: &str) -> anyhow::Result<(&str, &str)> {
    if contains_protocol(url) {
        let protocol = url.split("://").next().unwrap();
        let host = url.split("://").last().unwrap();

        if !VALID_PROTOCOLS.contains(&protocol) {
            Err(anyhow::anyhow!("Invalid protocol: {}", protocol))
        } else {
            Ok((host, protocol))
        }
    } else {
        Err(anyhow::anyhow!("Invalid url: {}", url))
    }
}

pub fn contains_protocol(name_or_url: &str) -> bool {
    name_or_url.contains("://")
}

pub fn host_or_url_to_host_and_protocol(host_or_url: &str) -> (&str, Option<&str>) {
    if contains_protocol(host_or_url) {
        let (host, protocol) = url_to_host_and_protocol(host_or_url).unwrap();
        (host, Some(protocol))
    } else {
        (host_or_url, None)
    }
}

/// Prompt the user for `y` or `n` from stdin.
///
/// Return `false` unless the input is `y`.
pub fn y_or_n(force: bool, prompt: &str) -> anyhow::Result<bool> {
    if force {
        println!("Skipping confirmation due to --yes");
        return Ok(true);
    }
    let mut input = String::new();
    print!("{} [y/N]", prompt);
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

pub fn unauth_error_context<T>(res: anyhow::Result<T>, identity: &str, server: &str) -> anyhow::Result<T> {
    res.with_context(|| {
        format!(
            "Identity {identity} is not valid for server {server}.
Please log back in with `spacetime logout` and then `spacetime login`."
        )
    })
}

pub fn decode_identity(token: &String) -> anyhow::Result<String> {
    // Here, we manually extract and decode the claims from the json web token.
    // We do this without using the `jsonwebtoken` crate because it doesn't seem to have a way to skip signature verification.
    // But signature verification would require getting the public key from a server, and we don't necessarily want to do that.
    let token_parts: Vec<_> = token.split('.').collect();
    if token_parts.len() != 3 {
        return Err(anyhow::anyhow!("Token does not look like a JSON web token: {}", token));
    }
    let decoded_bytes = BASE_64_STD_NO_PAD.decode(token_parts[1])?;
    let decoded_string = String::from_utf8(decoded_bytes)?;

    let claims_data: IncomingClaims = serde_json::from_str(decoded_string.as_str())?;
    let claims_data: SpacetimeIdentityClaims = claims_data.try_into()?;

    Ok(claims_data.identity.to_string())
}

pub async fn get_login_token_or_log_in(
    config: &mut Config,
    target_server: Option<&str>,
    interactive: bool,
) -> anyhow::Result<String> {
    if let Some(token) = config.spacetimedb_token() {
        return Ok(token.clone());
    }

    // Note: We pass `force: false` to `y_or_n` because if we're running non-interactively we want to default to "no", not yes!
    let full_login = interactive
        && y_or_n(
            false,
            // It would be "ideal" if we could print the `spacetimedb.com` by deriving it from the `default_auth_host` constant,
            // but this will change _so_ infrequently that it's not even worth the time to write that code and test it.
            "You are not logged in. Would you like to log in with spacetimedb.com?",
        )?;

    if full_login {
        let host = Url::parse(DEFAULT_AUTH_HOST)?;
        spacetimedb_login_force(config, &host, false).await
    } else {
        let host = Url::parse(&config.get_host_url(target_server)?)?;
        spacetimedb_login_force(config, &host, true).await
    }
}
