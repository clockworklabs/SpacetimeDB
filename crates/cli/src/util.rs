use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE_64_STD, Engine as _};
use reqwest::RequestBuilder;
use serde::Deserialize;
use spacetimedb_lib::name::{DnsLookupResponse, RegisterTldResult, ReverseDNSResponse};
use spacetimedb_lib::{Address, AlgebraicType, Identity};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::config::{Config, IdentityConfig};

/// Determine the address of the `database`.
pub async fn database_address(config: &Config, database: &str, server: Option<&str>) -> Result<Address, anyhow::Error> {
    if let Ok(address) = Address::from_hex(database) {
        return Ok(address);
    }
    match spacetime_dns(config, database, server).await? {
        DnsLookupResponse::Success { domain: _, address } => Ok(address),
        DnsLookupResponse::Failure { domain } => Err(anyhow::anyhow!("The dns resolution of `{}` failed.", domain)),
    }
}

/// Converts a name to a database address.
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
    identity: Option<&String>,
    server: Option<&str>,
) -> Result<RegisterTldResult, anyhow::Error> {
    let auth_header = get_auth_header_only(config, false, identity, server).await.unwrap();

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

/// Returns all known names for the given address.
pub async fn spacetime_reverse_dns(
    config: &Config,
    address: &str,
    server: Option<&str>,
) -> Result<ReverseDNSResponse, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("{}/database/reverse_dns/{}", config.get_host_url(server)?, address);
    let res = client.get(url).send().await?.error_for_status()?;
    let bytes = res.bytes().await.unwrap();
    Ok(serde_json::from_slice(&bytes[..]).unwrap())
}

#[derive(Deserialize)]
pub struct IdentityTokenJson {
    pub identity: Identity,
    pub token: String,
}

pub enum InitDefaultResultType {
    Existing,
    SavedNew,
}

pub struct InitDefaultResult {
    pub identity_config: IdentityConfig,
    pub result_type: InitDefaultResultType,
}

pub async fn init_default(
    config: &mut Config,
    nickname: Option<String>,
    server: Option<&str>,
) -> Result<InitDefaultResult, anyhow::Error> {
    if config.name_exists(nickname.as_ref().unwrap_or(&"".to_string())) {
        return Err(anyhow::anyhow!("A default identity already exists."));
    }

    let client = reqwest::Client::new();
    let builder = client.post(format!("{}/identity", config.get_host_url(server)?));

    if let Ok(identity_config) = config.get_default_identity_config(server) {
        return Ok(InitDefaultResult {
            identity_config: identity_config.clone(),
            result_type: InitDefaultResultType::Existing,
        });
    }

    let res = builder.send().await?;
    let res = res.error_for_status()?;

    let body = res.bytes().await?;
    let body = String::from_utf8(body.to_vec())?;

    let identity_token: IdentityTokenJson = serde_json::from_str(&body)?;

    let identity = identity_token.identity;

    let identity_config = IdentityConfig {
        identity: identity_token.identity,
        token: identity_token.token,
        nickname: nickname.clone(),
    };
    config.identity_configs_mut().push(identity_config.clone());
    if config.default_identity(server).is_err() {
        config.set_default_identity(identity.to_hex().to_string(), server)?;
    }
    config.save();
    Ok(InitDefaultResult {
        identity_config,
        result_type: InitDefaultResultType::SavedNew,
    })
}

/// Selects an `identity_config` from the config file. If you specify the
/// identity it will either return the `identity_config` for the specified
/// identity, or return an error if it cannot be found.  If you do not specify
/// an identity this function will either get the default identity if one exists
/// or create and save a new default identity.

// TODO: validate identity by server's public key
pub async fn select_identity_config(
    config: &mut Config,
    identity_or_name: Option<&str>,
    server: Option<&str>,
) -> Result<IdentityConfig, anyhow::Error> {
    if let Some(identity_or_name) = identity_or_name {
        config
            .get_identity_config(identity_or_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No such identity credentials for identity: {}", identity_or_name))
    } else {
        Ok(init_default(config, None, server).await?.identity_config)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DescribeReducer {
    #[serde(rename = "type")]
    pub type_field: String,
    pub arity: i32,
    pub schema: DescribeSchema,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DescribeSchema {
    pub name: String,
    pub elements: Vec<DescribeElement>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DescribeElement {
    pub name: Option<DescribeElementName>,
    pub algebraic_type: AlgebraicType,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DescribeElementName {
    pub some: String,
}

pub async fn describe_reducer(
    config: &mut Config,
    database: Address,
    server: Option<String>,
    reducer_name: String,
    anon_identity: bool,
    as_identity: Option<String>,
) -> anyhow::Result<DescribeReducer> {
    let builder = reqwest::Client::new().get(format!(
        "{}/database/schema/{}/{}/{}",
        config.get_host_url(server.as_deref())?,
        database,
        "reducer",
        reducer_name
    ));
    let auth_header = get_auth_header_only(config, anon_identity, as_identity.as_ref(), server.as_deref()).await?;
    let builder = add_auth_header_opt(builder, &auth_header);

    let descr = builder
        .query(&[("expand", true)])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let result: HashMap<String, DescribeReducer> = serde_json::from_str(descr.as_str()).unwrap();
    Ok(result[&reducer_name].clone())
}

/// Add an authorization header, if provided, to the request `builder`.
pub fn add_auth_header_opt(mut builder: RequestBuilder, auth_header: &Option<String>) -> RequestBuilder {
    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }
    builder
}

/// See [`get_auth_header`].
pub async fn get_auth_header_only(
    config: &mut Config,
    anon_identity: bool,
    identity_or_name: Option<&String>,
    server: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let (ah, _) = get_auth_header(config, anon_identity, identity_or_name.map(String::as_str), server)
        .await?
        .unzip();
    Ok(ah)
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
    identity_or_name: Option<&str>,
    server: Option<&str>,
) -> anyhow::Result<Option<(String, Identity)>> {
    Ok(if !anon_identity {
        let identity_config = select_identity_config(config, identity_or_name, server).await?;
        // The current form is: Authorization: Basic base64("token:<token>")
        let mut auth_header = String::new();
        auth_header.push_str(
            format!(
                "Basic {}",
                BASE_64_STD.encode(format!("token:{}", identity_config.token))
            )
            .as_str(),
        );
        Some((auth_header, identity_config.identity))
    } else {
        None
    })
}

pub fn is_hex_identity(ident: &str) -> bool {
    ident.len() == 64 && ident.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn print_identity_config(ident: &IdentityConfig) {
    println!(" IDENTITY  {}", ident.identity);
    println!(
        " NAME      {}",
        match &ident.nickname {
            None => "",
            Some(name) => name.as_str(),
        }
    );
    // TODO: lookup email here when we have an API endpoint for it
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

pub fn detect_module_language(path_to_project: &Path) -> ModuleLanguage {
    // TODO: Possible add a config file durlng spacetime init with the language
    // check for Cargo.toml
    if path_to_project.join("Cargo.toml").exists() {
        ModuleLanguage::Rust
    } else {
        ModuleLanguage::Csharp
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
pub fn y_or_n(prompt: &str) -> anyhow::Result<bool> {
    let mut input = String::new();
    print!("{} (y/n)", prompt);
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;

    Ok(input.trim() == "y")
}

pub fn unauth_error_context<T>(res: anyhow::Result<T>, identity: &str, server: &str) -> anyhow::Result<T> {
    res.with_context(|| {
        format!(
            "Identity {identity} is not valid for server {server}.
Has the server rotated its keys?
Remove the outdated identity with:
\tspacetime identity remove {identity}
Generate a new identity with:
\tspacetime identity new --no-email --server {server}"
        )
    })
}
