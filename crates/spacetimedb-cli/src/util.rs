use std::process::exit;

use clap::{
    error::{ContextKind, ContextValue},
    ArgMatches, Command,
};
use serde::{Deserialize, Serialize};

use crate::config::Config;

pub fn match_subcommand_or_exit(command: Command<'static>) -> (String, ArgMatches) {
    let mut command_clone = command.clone();
    let result = command.try_get_matches();
    let args = match result {
        Ok(args) => args,
        Err(e) => match e.kind() {
            clap::ErrorKind::UnknownArgument => {
                e.exit();
            }
            clap::ErrorKind::UnrecognizedSubcommand => {
                e.exit();
            }
            clap::ErrorKind::InvalidSubcommand => {
                e.exit();
            }
            clap::ErrorKind::MissingSubcommand => {
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
