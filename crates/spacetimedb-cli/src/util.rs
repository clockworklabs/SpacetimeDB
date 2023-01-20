use std::process::exit;

use clap::{
    error::{ContextKind, ContextValue},
    ArgMatches, Command,
};
use serde::{Deserialize, Serialize};

use crate::config::{Config, IdentityConfig};

pub fn match_subcommand_or_exit(command: Command) -> (String, ArgMatches) {
    let mut command_clone = command.clone();
    let result = command.try_get_matches();
    let args = match result {
        Ok(args) => args,
        Err(e) => match e.kind() {
            clap::error::ErrorKind::MissingSubcommand => {
                let cmd = e
                    .context()
                    .find_map(|c| match c {
                        (ContextKind::InvalidSubcommand, &ContextValue::String(ref cmd)) => {
                            Some(cmd.split_ascii_whitespace().last().unwrap())
                        }
                        _ => None,
                    })
                    .expect("The InvalidArg to be present in the context of UnknownArgument.");
                match command_clone.find_subcommand_mut(cmd) {
                    Some(subcmd) => subcmd.print_help().unwrap(),
                    None => command_clone.print_help().unwrap(),
                }
                exit(0);
            }
            _ => {
                e.exit();
            }
        },
    };
    let (cmd, subcommand_args) = args.subcommand().unwrap();
    (cmd.to_string(), subcommand_args.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DNSResponse {
    address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReverseDNSResponse {
    name: String,
}

pub async fn spacetime_dns(config: &Config, domain_name: &str) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("{}/database/dns/{}", config.get_host_url(), domain_name);
    let res = client.get(url).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let dns: DNSResponse = serde_json::from_slice(&bytes[..]).unwrap();
    Ok(dns.address)
}

pub async fn spacetime_reverse_dns(config: &Config, addr: &str) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("{}/database/reverse_dns/{}", config.get_host_url(), addr);
    let res = client.get(url).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let dns: ReverseDNSResponse = serde_json::from_slice(&bytes[..]).unwrap();
    Ok(dns.name)
}

#[derive(Deserialize)]
pub struct IdentityTokenJson {
    pub identity: String,
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

pub async fn init_default(config: &mut Config, nickname: Option<String>) -> Result<InitDefaultResult, anyhow::Error> {
    if config.name_exists(nickname.as_ref().unwrap_or(&"".to_string())) {
        println!("An identity with that name already exists.");
        std::process::exit(0);
    }

    let client = reqwest::Client::new();
    let builder = client.post(format!("{}/identity", config.get_host_url()));

    if let Some(identity_config) = config.get_default_identity_config() {
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

    let identity = identity_token.identity.clone();

    let identity_config = IdentityConfig {
        identity: identity_token.identity,
        token: identity_token.token,
        nickname: nickname.clone(),
        email: None,
    };
    config.identity_configs.push(identity_config.clone());
    if config.default_identity.is_none() {
        config.default_identity = Some(identity);
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
pub async fn select_identity_config(
    config: &mut Config,
    identity: Option<&str>,
) -> Result<IdentityConfig, anyhow::Error> {
    if let Some(identity) = identity {
        if let Some(identity_config) = config.get_identity_config_by_identity(&identity) {
            return Ok(identity_config.clone());
        } else {
            return Err(anyhow::anyhow!(format!(
                "Missing identity credentials for identity: {}",
                identity
            )));
        }
    } else {
        return Ok(init_default(config, None).await?.identity_config);
    }
}

/// Gets the `auth_header` for a request to the server depending on how you want
/// to identify yourself.  If you specify `anon_identity = true` then no
/// `auth_header` is returned. If you specify an identity this function will try
/// to find the identity in the config file. If no identity can be found, the
/// program will `exit(1)`. If you do not specify an identity this function will
/// either get the default identity if one exists or create and save a new
/// default identity returning the one that was just created.
pub async fn get_auth_header(config: &mut Config, anon_identity: bool, identity: Option<&str>) -> Option<String> {
    if !anon_identity {
        let identity_config = match select_identity_config(config, identity).await {
            Ok(ic) => ic,
            Err(err) => {
                println!("{}", err.to_string());
                std::process::exit(1);
            }
        };
        // The current form is: Authorization: Basic base64("token:<token>")
        let mut auth_header = String::new();
        auth_header.push_str(format!("Basic {}", base64::encode(format!("token:{}", identity_config.token))).as_str());
        Some(auth_header)
    } else {
        None
    }
}
