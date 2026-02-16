use anyhow::Context;
use clap::Arg;
use clap::ArgAction::{Set, SetTrue};
use clap::ArgMatches;
use reqwest::{StatusCode, Url};
use spacetimedb_client_api_messages::name::{is_identity, parse_domain_name, PublishResult};
use spacetimedb_client_api_messages::name::{PrePublishResult, PrettyPrintStyle, PublishOp};
use std::path::PathBuf;
use std::{env, fs};

use crate::common_args::ClearMode;
use crate::config::Config;
use crate::util::{add_auth_header_opt, get_auth_header, prepend_root_database_namespace, AuthHeader, ResponseExt};
use crate::util::{decode_identity, y_or_n};
use crate::{build, common_args};

pub fn cli() -> clap::Command {
    clap::Command::new("publish")
        .about("Create and update a SpacetimeDB database")
        .arg(
            common_args::clear_database()
                .requires("name|identity")
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
                .help("Allow breaking changes when publishing to an existing database identity. This will force publish even if it will break existing clients, but will NOT force publish if it would cause deletion of any data in the database. See --yes and --delete-data for details.")
        )
        .arg(
            common_args::anonymous()
        )
        .arg(
            Arg::new("parent")
            .help("Database name or identity of a parent for this database")
            .long("parent")
            .long_help(
"A valid database name or identity of an existing database that should be the parent of this database.

If a parent is given, the new database inherits the team permissions from the parent.
A parent can only be set when a database is created, not when it is updated."
            )
        )
        .arg(
            Arg::new("organization")
            .help("Name or identity of an organization for this database")
            .long("organization")
            .alias("org")
            .long_help(
"The name or identity of an existing organization this database should be created under.

If an organization is given, the organization member's permissions apply to the new database.
An organization can only be set when a database is created, not when it is updated."
            )
        )
        .arg(
            Arg::new("name|identity")
                .help("A valid database name or identity for this database")
                .long_help(
"A valid database name or identity for this database.

Database names may include a root namespace and child path segments,
for example: `@user/my-db` or `@user/game/region-1`."),
        )
        .arg(common_args::server()
                .help("The nickname, domain name or URL of the server to host the database."),
        )
        .arg(
            common_args::yes()
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

fn confirm_and_clear(
    name_or_identity: &str,
    force: bool,
    mut builder: reqwest::RequestBuilder,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    println!(
        "This will DESTROY the current {} module, and ALL corresponding data.",
        name_or_identity
    );
    if !y_or_n(
        force,
        format!("Are you sure you want to proceed? [deleting {}]", name_or_identity).as_str(),
    )? {
        anyhow::bail!("Aborted.");
    }

    builder = builder.query(&[("clear", true)]);
    Ok(builder)
}

fn confirm_major_version_upgrade() -> Result<(), anyhow::Error> {
    println!(
        "It looks like you're trying to do a major version upgrade from 1.0 to 2.0. We recommend first looking at the upgrade notes before committing to this upgrade: https://spacetimedb.com/docs/upgrade"
    );
    println!();
    println!("WARNING: Once you publish you cannot revert back to version 1.0.");
    println!();

    let mut input = String::new();
    print!("Please type 'upgrade' to accept this change: ");
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout)?;
    std::io::stdin().read_line(&mut input)?;

    if input.trim() == "upgrade" {
        return Ok(());
    }

    anyhow::bail!("Aborting because major version upgrade was not accepted.");
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_str());
    let name_or_identity = args.get_one::<String>("name|identity");
    let path_to_project = args.get_one::<PathBuf>("project_path").unwrap();
    let clear_database = args
        .get_one::<ClearMode>("clear-database")
        .copied()
        .unwrap_or(ClearMode::Never);
    let force = args.get_flag("force");
    let anon_identity = args.get_flag("anon_identity");
    let wasm_file = args.get_one::<PathBuf>("wasm_file");
    let js_file = args.get_one::<PathBuf>("js_file");
    let database_host = config.get_host_url(server)?;
    let build_options = args.get_one::<String>("build_options").unwrap();
    let num_replicas = args.get_one::<u8>("num_replicas");
    let force_break_clients = args.get_flag("break_clients");
    let parent = args.get_one::<String>("parent");
    let org = args.get_one::<String>("organization");

    // If the user didn't specify an identity and we didn't specify an anonymous identity, then
    // we want to use the default identity
    // TODO(jdetter): We should maybe have some sort of user prompt here for them to be able to
    //  easily create a new identity with an email
    let auth_header = get_auth_header(&mut config, anon_identity, server, !force).await?;

    let root_database_namespace = config.root_database_namespace();
    let (name_or_identity, parent) = normalize_name_and_parent(
        name_or_identity.map(String::as_str),
        parent.map(String::as_str),
        root_database_namespace,
    )?;

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
    // If a name was given, ensure to percent-encode it.
    // We also use PUT with a name or identity, and POST otherwise.
    let mut builder = if let Some(name_or_identity) = name_or_identity.as_deref() {
        let encode_set = const { &percent_encoding::NON_ALPHANUMERIC.remove(b'_').remove(b'-') };
        let domain = percent_encoding::percent_encode(name_or_identity.as_bytes(), encode_set);
        let mut builder = client.put(format!("{database_host}/v1/database/{domain}"));

        // note that this only happens in the case where we've passed a `name_or_identity`, but that's required if we pass `--clear-database`.
        if clear_database == ClearMode::Always {
            builder = confirm_and_clear(name_or_identity, force, builder)?;
        } else {
            builder = apply_pre_publish_if_needed(
                builder,
                &client,
                &database_host,
                name_or_identity,
                &domain.to_string(),
                host_type,
                &program_bytes,
                &auth_header,
                clear_database,
                force_break_clients,
                force,
            )
            .await?;
        }

        builder
    } else {
        client.post(format!("{database_host}/v1/database"))
    };

    if let Some(n) = num_replicas {
        eprintln!("WARNING: Use of unstable option `--num-replicas`.\n");
        builder = builder.query(&[("num_replicas", *n)]);
    }
    if let Some(parent) = parent {
        builder = builder.query(&[("parent", parent)]);
    }
    if let Some(org) = org {
        builder = builder.query(&[("org", org)]);
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
                println!("Connection database name: {domain}");
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
            let suggested_namespace = config
                .root_database_namespace()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("@{}", identity.chars().take(12).collect::<String>()));
            return Err(anyhow::anyhow!(
                "The database name {name} is not registered to the identity you provided.\n\
                Publish under a root namespace that you own, for example:\n\
                \tspacetime publish {suggested_namespace}/my-database\n",
            ));
        }
    }

    Ok(())
}

fn validate_name_or_identity(name_or_identity: &str) -> anyhow::Result<()> {
    if is_identity(name_or_identity) {
        Ok(())
    } else {
        parse_domain_name(name_or_identity)
            .map(drop)
            .map_err(anyhow::Error::from)
    }
}

fn invalid_parent_name(name: &str) -> String {
    format!("invalid parent database name `{name}`")
}

fn normalize_name_and_parent(
    name: Option<&str>,
    parent: Option<&str>,
    root_database_namespace: Option<&str>,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let mut name = name.map(str::to_owned);
    let mut parent = parent.map(str::to_owned);

    if let Some(parent_name) = parent.as_deref() {
        validate_name_or_identity(parent_name).with_context(|| invalid_parent_name(parent_name))?;
    }

    if parent.is_some()
        && name
            .as_deref()
            .is_some_and(|raw_name| !is_identity(raw_name) && raw_name.contains('/'))
    {
        anyhow::bail!("child database name cannot contain `/` when --parent is set");
    }

    if let Some(parent_name) = parent.as_mut() {
        *parent_name = prepend_root_database_namespace(parent_name, root_database_namespace);
        validate_name_or_identity(parent_name).with_context(|| invalid_parent_name(parent_name))?;
    }

    if let Some(name_or_identity) = name.as_mut() {
        if parent.is_none() {
            *name_or_identity = prepend_root_database_namespace(name_or_identity, root_database_namespace);
        }
        validate_name_or_identity(name_or_identity)?;
    }

    Ok((name, parent))
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
    name_or_identity: &str,
    domain: &String,
    host_type: &str,
    program_bytes: &[u8],
    auth_header: &AuthHeader,
    clear_database: ClearMode,
    force_break_clients: bool,
    force: bool,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    // The caller enforces this
    assert!(clear_database != ClearMode::Always);

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
        let major_version_upgrade = match &pre {
            PrePublishResult::AutoMigrate(auto) => auto.major_version_upgrade,
            PrePublishResult::ManualMigrate(manual) => manual.major_version_upgrade,
        };
        if major_version_upgrade {
            confirm_major_version_upgrade()?;
        }

        match pre {
            PrePublishResult::ManualMigrate(manual) => {
                if clear_database == ClearMode::Never {
                    println!("{}", manual.reason);
                    println!("Aborting publish due to required manual migration.");
                    anyhow::bail!("Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified.");
                }
                println!("{}", manual.reason);
                println!("Proceeding with database clear due to --delete-data=on-conflict.");

                builder = confirm_and_clear(name_or_identity, force, builder)?;
            }
            PrePublishResult::AutoMigrate(auto) => {
                println!("{}", auto.migrate_plan);
                // We only arrive here if you have not specified ClearMode::Always AND there was no
                // conflict that required manual migration.
                if auto.break_clients
                    && !y_or_n(
                        force_break_clients || force,
                        "The above changes will BREAK existing clients. Do you want to proceed?",
                    )?
                {
                    println!("Aborting");
                    // Early exit: return an error or a special signal. Here we bail out by returning Err.
                    anyhow::bail!("Publishing aborted by user");
                }
                builder = builder
                    .query(&[("token", auto.token)])
                    .query(&[("policy", "BreakClients")]);
            }
        }
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
    let style: PrettyPrintStyle = pretty_print_style_from_env();
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_matches;
    use spacetimedb_lib::Identity;

    use super::*;

    #[test]
    fn validate_none_arguments_returns_none_values() {
        assert_matches!(normalize_name_and_parent(None, None, None), Ok((None, None)));
        assert_matches!(normalize_name_and_parent(Some("foo"), None, None), Ok((Some(_), None)));
        assert_matches!(normalize_name_and_parent(None, Some("foo"), None), Ok((None, Some(_))));
    }

    #[test]
    fn validate_valid_arguments_returns_arguments() {
        let name = "child";
        let parent = "parent";
        let result = (Some(name.to_owned()), Some(parent.to_owned()));
        assert_matches!(
            normalize_name_and_parent(Some(name), Some(parent), None),
            Ok(val) if val == result
        );
    }

    #[test]
    fn validate_path_name_is_rejected_when_parent_is_set() {
        assert_matches!(
            normalize_name_and_parent(Some("parent/child"), Some("parent"), None),
            Err(_)
        );
        assert_matches!(
            normalize_name_and_parent(Some("parent/child"), Some("cousin"), None),
            Err(_)
        );
    }

    #[test]
    fn validate_more_than_two_path_segments_are_supported() {
        assert_matches!(
            normalize_name_and_parent(Some("proc/net/tcp"), None, None),
            Ok((Some(name), None)) if name == "proc/net/tcp"
        );
        assert_matches!(normalize_name_and_parent(Some("proc//net"), None, None), Err(_));
    }

    #[test]
    fn validate_trailing_slash_is_an_error() {
        assert_matches!(normalize_name_and_parent(Some("foo//"), None, None), Err(_));
        assert_matches!(normalize_name_and_parent(Some("foo/bar/"), None, None), Err(_));
    }

    #[test]
    fn validate_parent_can_have_path_segments() {
        assert_matches!(
            normalize_name_and_parent(Some("child"), Some("par/ent"), None),
            Ok((Some(name), Some(parent))) if name == "child" && parent == "par/ent"
        );
        assert_matches!(normalize_name_and_parent(Some("child"), Some("parent/"), None), Err(_));
    }

    #[test]
    fn validate_name_or_parent_can_be_identities() {
        let parent = Identity::ZERO.to_string();
        let child = Identity::ONE.to_string();

        assert_matches!(
            normalize_name_and_parent(Some(&child), Some(&parent), None),
            Ok((Some(name), Some(parent_name))) if name == child && parent_name == parent
        );
    }

    #[test]
    fn prepend_root_namespace_to_unqualified_name_and_parent() {
        assert_matches!(
            normalize_name_and_parent(Some("my-db"), None, Some("@alice")),
            Ok((Some(name), None)) if name == "@alice/my-db"
        );
        assert_matches!(
            normalize_name_and_parent(Some("child"), Some("parent/leaf"), Some("@alice")),
            Ok((Some(name), Some(parent))) if name == "child" && parent == "@alice/parent/leaf"
        );
        assert_matches!(
            normalize_name_and_parent(Some("parent/leaf/child"), None, Some("@alice")),
            Ok((Some(name), None)) if name == "@alice/parent/leaf/child"
        );
    }

    #[test]
    fn dont_prepend_root_namespace_when_already_qualified_or_identity() {
        let identity = Identity::ZERO.to_string();
        assert_matches!(
            normalize_name_and_parent(Some("@bob/my-db"), None, Some("@alice")),
            Ok((Some(name), None)) if name == "@bob/my-db"
        );
        assert_matches!(
            normalize_name_and_parent(Some(&identity), None, Some("@alice")),
            Ok((Some(name), None)) if name == identity
        );
    }
}
