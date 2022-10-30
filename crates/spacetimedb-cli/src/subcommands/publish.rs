use clap::Arg;
use clap::ArgMatches;
use duckscript::types::runtime::{Context, StateValue};
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::config::Config;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("publish")
        .about("Create and update a SpacetimeDB database.")
        .arg(
            Arg::new("host_type")
                .takes_value(true)
                .required(false)
                .long("host_type")
                .short('t')
                .possible_values(["wasmer"]),
        )
        .arg(
            Arg::new("clear_database")
                .long("clear-database")
                .short('c')
                .takes_value(false),
        )
        .arg(
            Arg::new("path_to_project")
                .required(false)
                .default_value(".")
                .long("project")
                .short('p'),
        )
        .arg(
            Arg::new("identity")
                .takes_value(true)
                .long("identity")
                .short('i')
                .required(false),
        )
        .arg(Arg::new("name|database").takes_value(true).required(false))
        .after_help("Run `spacetime help publish` for more detailed information.")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitDatabaseResponse {
    address: String,
}

fn is_address(input: &str) -> bool {
    return match hex::decode(input) {
        Ok(hex) => hex.len() == 16,
        Err(_) => false,
    };
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let mut url_args = String::new();

    // Identity is required
    if let Some(identity) = args.value_of("identity") {
        url_args.push_str(format!("?identity={}", identity).as_str());
    } else {
        let identity_config = config.get_default_identity_config().unwrap();
        url_args.push_str(format!("?identity={}", identity_config.identity).as_str());
    }

    let name_or_address = args.value_of("name|database");
    if let Some(name_or_address) = name_or_address {
        if is_address(name_or_address) {
            url_args.push_str(format!("&address={}", name_or_address).as_str());
        } else {
            url_args.push_str(format!("&name={}", name_or_address).as_str());
        }
    }

    let path_to_project_str = args.value_of("path_to_project").unwrap();
    if path_to_project_str.trim().is_empty() {
        return Err(anyhow::anyhow!("Project path is required!"));
    }
    let path_to_project = Path::new(path_to_project_str);
    if !path_to_project.exists() {
        return Err(anyhow::anyhow!("Project path does not exist: {}", path_to_project_str));
    }

    let module_output_directory_path = path_to_project
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release");
    if !module_output_directory_path.exists() || !module_output_directory_path.is_dir() {
        return Err(anyhow::anyhow!(
            "Module output directory does not exist: {}",
            module_output_directory_path.to_str().unwrap()
        ));
    }

    let mut wasm_file_path = None;
    for file in fs::read_dir(module_output_directory_path.to_str().unwrap()).unwrap() {
        match file {
            Ok(f) => {
                if f.file_name().to_str().unwrap().ends_with(".wasm") {
                    wasm_file_path = Some(f.path());
                    break;
                }
            }
            Err(_) => {}
        }
    }

    if let None = wasm_file_path {
        return Err(anyhow::anyhow!("Unable to find wasm output!"));
    }

    let path_to_wasm = wasm_file_path.unwrap();
    if !path_to_wasm.exists() {
        return Err(anyhow::anyhow!(
            "Unable to find wasm module: {}",
            path_to_wasm.to_str().unwrap()
        ));
    }
    if !path_to_wasm.is_file() {
        return Err(anyhow::anyhow!(
            "Path to wasm file isn't a file: {}",
            path_to_wasm.to_str().unwrap()
        ));
    }
    let path_to_wasm = path_to_wasm.to_str().unwrap();

    if args.is_present("clear_database") {
        url_args.push_str("&clear=true");
    }

    url_args.push_str(format!("&host_type={}", args.value_of("host_type").unwrap_or("wasmer")).as_str());
    let program_bytes = fs::read(fs::canonicalize(path_to_wasm).unwrap())?;

    let mut context = Context::new();
    duckscriptsdk::load(&mut context.commands)?;
    context
        .variables
        .insert("PATH".to_string(), std::env::var("PATH").unwrap());
    context
        .variables
        .insert("PROJECT_PATH".to_string(), path_to_project_str.to_string());

    match duckscript::runner::run_script(include_str!("project/pre-publish.duck"), context) {
        Ok(ok) => {
            let mut error = false;
            for entry in ok.state {
                if let StateValue::SubState(sub_state) = entry.1 {
                    for entry in sub_state {
                        match entry.1 {
                            StateValue::String(a) => {
                                error = true;
                                println!("{}|{}", entry.0, a)
                            }
                            _ => {}
                        }
                    }
                }
            }

            if !error {
                println!("Publish finished successfully.");
            } else {
                println!("Publish finished with errors, check the console for more information.");
            }
        }
        Err(e) => {
            println!("Script execution error: {}", e);
        }
    }

    let url = format!("http://{}/database/publish{}", config.host, url_args);
    let client = reqwest::Client::new();
    let res = client.post(url).body(program_bytes).send().await?;
    let res = res.error_for_status()?;
    let bytes = res.bytes().await.unwrap();

    let response: InitDatabaseResponse = serde_json::from_slice(&bytes[..]).unwrap();
    println!("Created new database with address: {}", response.address);

    Ok(())
}
