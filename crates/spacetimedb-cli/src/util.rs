use std::fs;
use std::path::{Path, PathBuf};
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

pub async fn spacetime_dns(config: &Config, domain_name: &str) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("http://{}/database/dns/{}", config.host, domain_name);
    let res = client.get(url).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let dns: DNSResponse = serde_json::from_slice(&bytes[..]).unwrap();
    Ok(dns.address)
}

pub fn find_wasm_file(project_path: &Path) -> Result<PathBuf, anyhow::Error> {
    let module_output_directory_path = project_path
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release");
    if !module_output_directory_path.exists() || !module_output_directory_path.is_dir() {
        return Err(anyhow::anyhow!(
            "Module output directory does not exist: {}",
            module_output_directory_path.to_str().unwrap()
        ));
    }

    for file in fs::read_dir(module_output_directory_path.to_str().unwrap())
        .unwrap()
        .flatten()
    {
        if file.file_name().to_str().unwrap().ends_with(".wasm") && file.path().is_file() {
            return Ok(file.path());
        }
    }

    Err(anyhow::anyhow!(format!(
        "Unable to find wasm file in project path: {}",
        project_path.to_str().unwrap()
    )))
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
    let builder = client.post(format!("http://{}/identity", config.host));

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
