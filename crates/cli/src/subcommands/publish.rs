use clap::Arg;
use clap::ArgAction::{Set, SetTrue};
use clap::ArgMatches;
use reqwest::{StatusCode, Url};
use spacetimedb_client_api_messages::name::{is_identity, parse_database_name, PublishResult};
use spacetimedb_client_api_messages::name::{PrePublishResult, PrettyPrintStyle, PublishOp};
use std::path::PathBuf;
use std::{env, fs};

use crate::config::Config;
use crate::util::{add_auth_header_opt, get_auth_header, AuthHeader, ResponseExt};
use crate::util::{decode_identity, unauth_error_context, y_or_n};
use crate::{build, common_args};

pub fn cli() -> clap::Command {
    clap::Command::new("publish")
        .about("Create and update a SpacetimeDB database")
        .arg(
            Arg::new("clear_database")
                .long("delete-data")
                .short('c')
                .action(SetTrue)
                .requires("name|identity")
                .help("When publishing to an existing database identity, first DESTROY all data associated with the module"),
        )
        .arg(
            Arg::new("build_options")
                .long("build-options")
                .alias("build-opts")
                .action(Set)
                .default_value("")
                .help("Options to pass to the build command, for example --build-options='--lint-dir='")
        )
        .arg(
            Arg::new("project_path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .long("project-path")
                .short('p')
                .help("The system path (absolute or relative) to the module project")
        )
        .arg(
            Arg::new("wasm_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("bin-path")
                .short('b')
                .conflicts_with("project_path")
                .conflicts_with("build_options")
                .conflicts_with("js_file")
                .help("The system path (absolute or relative) to the compiled wasm binary we should publish, instead of building the project."),
        )
        .arg(
            Arg::new("js_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("js-path")
                .short('j')
                .conflicts_with("project_path")
                .conflicts_with("build_options")
                .conflicts_with("wasm_file")
                .help("UNSTABLE: The system path (absolute or relative) to the javascript file we should publish, instead of building the project."),
        )
        .arg(
            Arg::new("num_replicas")
                .value_parser(clap::value_parser!(u8))
                .long("num-replicas")
                .hide(true)
                .help("UNSTABLE: The number of replicas the database should have")
        )
        .arg(
            Arg::new("break_clients")
                .long("break-clients")
                .action(SetTrue)
                .help("Allow breaking changes when publishing to an existing database identity. This will break existing clients.")
        )
        .arg(
            common_args::anonymous()
        )
        .arg(
            Arg::new("name|identity")
                .help("A valid domain or identity for this database")
                .long_help(
"A valid domain or identity for this database.

Database names must match the regex `/^[a-z0-9]+(-[a-z0-9]+)*$/`,
i.e. only lowercase ASCII letters and numbers, separated by dashes."),
        )
        .arg(common_args::server()
                .help("The nickname, domain name or URL of the server to host the database."),
        )
        .arg(
            common_args::yes()
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_str());
    let name_or_identity = args.get_one::<String>("name|identity");
    let path_to_project = args.get_one::<PathBuf>("project_path").unwrap();
    let clear_database = args.get_flag("clear_database");
    let force = args.get_flag("force");
    let anon_identity = args.get_flag("anon_identity");
    let wasm_file = args.get_one::<PathBuf>("wasm_file");
    let js_file = args.get_one::<PathBuf>("js_file");
    let database_host = config.get_host_url(server)?;
    let build_options = args.get_one::<String>("build_options").unwrap();
    let num_replicas = args.get_one::<u8>("num_replicas");
    let break_clients_flag = args.get_flag("break_clients");

    // If the user didn't specify an identity and we didn't specify an anonymous identity, then
    // we want to use the default identity
    // TODO(jdetter): We should maybe have some sort of user prompt here for them to be able to
    //  easily create a new identity with an email
    let auth_header = get_auth_header(&mut config, anon_identity, server, !force).await?;

    if !path_to_project.exists() {
        return Err(anyhow::anyhow!(
            "Project path does not exist: {}",
            path_to_project.display()
        ));
    }

    // Decide program file path and read program.
    // Optionally build the program.
    let (path_to_program, host_type) = if let Some(path) = wasm_file {
        println!("(WASM) Skipping build. Instead we are publishing {}", path.display());
        (path.clone(), "Wasm")
    } else if let Some(path) = js_file {
        println!("(JS) Skipping build. Instead we are publishing {}", path.display());
        (path.clone(), "Js")
    } else {
        build::exec_with_argstring(config.clone(), path_to_project, build_options).await?
    };
    let program_bytes = fs::read(path_to_program)?;

    let server_address = {
        let url = Url::parse(&database_host)?;
        url.host_str().unwrap_or("<default>").to_string()
    };
    if server_address != "localhost" && server_address != "127.0.0.1" {
        println!("You are about to publish to a non-local server: {server_address}");
        if !y_or_n(force, "Are you sure you want to proceed?")? {
            println!("Aborting");
            return Ok(());
        }
    }

    println!(
        "Uploading to {} => {}",
        server.unwrap_or(config.default_server_name().unwrap_or("<default>")),
        database_host
    );

    let client = reqwest::Client::new();
    // If a domain or identity was provided, we should locally make sure it looks correct and
    let mut builder = if let Some(name_or_identity) = name_or_identity {
        if !is_identity(name_or_identity) {
            parse_database_name(name_or_identity)?;
        }
        let encode_set = const { &percent_encoding::NON_ALPHANUMERIC.remove(b'_').remove(b'-') };
        let domain = percent_encoding::percent_encode(name_or_identity.as_bytes(), encode_set);

        let mut builder = client.put(format!("{database_host}/v1/database/{domain}"));

        if !clear_database {
            builder = apply_pre_publish_if_needed(
                builder,
                &client,
                &database_host,
                &domain.to_string(),
                host_type,
                &program_bytes,
                &auth_header,
                break_clients_flag,
            )
            .await?;
        };

        builder
    } else {
        client.post(format!("{database_host}/v1/database"))
    };

    if clear_database {
        // Note: `name_or_identity` should be set, because it is `required` in the CLI arg config.
        println!(
            "This will DESTROY the current {} module, and ALL corresponding data.",
            name_or_identity.unwrap()
        );
        if !y_or_n(
            force,
            format!(
                "Are you sure you want to proceed? [deleting {}]",
                name_or_identity.unwrap()
            )
            .as_str(),
        )? {
            println!("Aborting");
            return Ok(());
        }
        builder = builder.query(&[("clear", true)]);
    }
    if let Some(n) = num_replicas {
        eprintln!("WARNING: Use of unstable option `--num-replicas`.\n");
        builder = builder.query(&[("num_replicas", *n)]);
    }

    println!("Publishing module...");

    builder = add_auth_header_opt(builder, &auth_header);

    // Set the host type.
    builder = builder.query(&[("host_type", host_type)]);

    // JS/TS is beta quality atm.
    if host_type == "Js" {
        println!("JavaScript / TypeScript support is currently in BETA.");
        println!("There may be bugs. Please file issues if you encounter any.");
        println!("<https://github.com/clockworklabs/SpacetimeDB/issues/new>");
    }

    let res = builder.body(program_bytes).send().await?;
    if res.status() == StatusCode::UNAUTHORIZED && !anon_identity {
        // If we're not in the `anon_identity` case, then we have already forced the user to log in above (using `get_auth_header`), so this should be safe to unwrap.
        let token = config.spacetimedb_token().unwrap();
        let identity = decode_identity(token)?;
        let err = res.text().await?;
        return unauth_error_context(
            Err(anyhow::anyhow!(err)),
            &identity,
            config.server_nick_or_host(server)?,
        );
    }

    let response: PublishResult = res.json_or_error().await?;
    match response {
        PublishResult::Success {
            domain,
            database_identity,
            op,
        } => {
            let op = match op {
                PublishOp::Created => "Created new",
                PublishOp::Updated => "Updated",
            };
            if let Some(domain) = domain {
                println!("{op} database with name: {domain}, identity: {database_identity}");
            } else {
                println!("{op} database with identity: {database_identity}");
            }
        }
        PublishResult::PermissionDenied { name } => {
            if anon_identity {
                anyhow::bail!("You need to be logged in as the owner of {name} to publish to {name}",);
            }
            // If we're not in the `anon_identity` case, then we have already forced the user to log in above (using `get_auth_header`), so this should be safe to unwrap.
            let token = config.spacetimedb_token().unwrap();
            let identity = decode_identity(token)?;
            //TODO(jdetter): Have a nice name generator here, instead of using some abstract characters
            // we should perhaps generate fun names like 'green-fire-dragon' instead
            let suggested_tld: String = identity.chars().take(12).collect();
            return Err(anyhow::anyhow!(
                "The database {name} is not registered to the identity you provided.\n\
                We suggest you push to either a domain owned by you, or a new domain like:\n\
                \tspacetime publish {suggested_tld}\n",
            ));
        }
    }

    Ok(())
}

/// Determine the pretty print style based on the NO_COLOR environment variable.
///
/// See: https://no-color.org
pub fn pretty_print_style_from_env() -> PrettyPrintStyle {
    match env::var("NO_COLOR") {
        Ok(_) => PrettyPrintStyle::NoColor,
        Err(_) => PrettyPrintStyle::AnsiColor,
    }
}

/// Applies pre-publish logic: checking for migration plan, prompting user, and
/// modifying the request builder accordingly.
#[allow(clippy::too_many_arguments)]
async fn apply_pre_publish_if_needed(
    mut builder: reqwest::RequestBuilder,
    client: &reqwest::Client,
    base_url: &str,
    domain: &String,
    host_type: &str,
    program_bytes: &[u8],
    auth_header: &AuthHeader,
    break_clients_flag: bool,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    if let Some(pre) = call_pre_publish(
        client,
        base_url,
        &domain.to_string(),
        host_type,
        program_bytes,
        auth_header,
    )
    .await?
    {
        println!("{}", pre.migrate_plan);

        if pre.break_clients
            && !y_or_n(
                break_clients_flag,
                "The above changes will BREAK existing clients. Do you want to proceed?",
            )?
        {
            println!("Aborting");
            // Early exit: return an error or a special signal. Here we bail out by returning Err.
            anyhow::bail!("Publishing aborted by user");
        }

        builder = builder
            .query(&[("token", pre.token)])
            .query(&[("policy", "BreakClients")]);
    }

    Ok(builder)
}

async fn call_pre_publish(
    client: &reqwest::Client,
    database_host: &str,
    domain: &String,
    host_type: &str,
    program_bytes: &[u8],
    auth_header: &AuthHeader,
) -> Result<Option<PrePublishResult>, anyhow::Error> {
    let mut builder = client.post(format!("{database_host}/v1/database/{domain}/pre_publish"));
    let style = pretty_print_style_from_env();
    builder = builder
        .query(&[("pretty_print_style", style)])
        .query(&[("host_type", host_type)]);

    builder = add_auth_header_opt(builder, auth_header);

    println!("Checking for breaking changes...");
    let res = builder.body(program_bytes.to_vec()).send().await?;

    if res.status() == StatusCode::NOT_FOUND {
        // This is a new database, so there are no breaking changes
        return Ok(None);
    }

    if !res.status().is_success() {
        anyhow::bail!(
            "Pre-publish check failed with status {}: {}",
            res.status(),
            res.text().await?
        );
    }

    let pre_publish_result: PrePublishResult = res.json_or_error().await?;
    Ok(Some(pre_publish_result))
}
