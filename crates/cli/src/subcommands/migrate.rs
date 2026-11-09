//! `spacetime migrate`: tools for manual migrations.

use crate::api::ClientApi;
use crate::common_args;
use crate::config::Config;
use crate::spacetime_config::find_and_load_with_env;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_arg};
use crate::subcommands::publish::default_publish_module_path;
use crate::subcommands::{build, generate};
use crate::util::{database_identity, get_auth_header, y_or_n};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches};
use dialoguer::{theme::ColorfulTheme, Input};
use spacetimedb_codegen::{DroppedSchema, Lang, Rust};
use spacetimedb_schema::def::ModuleDef;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

const GENERATE_USAGE: &str = "spacetime migrate generate [database] [--no-config]";

pub fn cli() -> clap::Command {
    clap::Command::new("migrate")
        .about("Manage manual migrations of a database's schema")
        .subcommand_required(true)
        .subcommand(generate_cli())
}

fn generate_cli() -> clap::Command {
    clap::Command::new("generate")
        .about("Generate a manual migration from a database's current schema to the module's schema")
        .long_about(
            "Generate a manual migration from a database's current schema to the module's schema.\n\n\
             Builds the module, compares its schema to the one the database is running, and writes \
             a Rust file declaring the tables the new schema removes or changes incompatibly, along \
             with a `migrate` function to fill in. Add the file to your module and publish it to \
             migrate the database's data to the new schema.",
        )
        .arg(
            Arg::new("database")
                .required(false)
                .help("The name or identity of the database to migrate"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(
            Arg::new("module_path")
                .long("module-path")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to the Rust module project. Defaults to the project's configured module path"),
        )
        .arg(
            Arg::new("wasm_file")
                .long("bin-path")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Read the new schema from this compiled module, instead of building the module"),
        )
        .arg(
            Arg::new("build_options")
                .long("build-options")
                .alias("build-opts")
                .action(ArgAction::Set)
                .default_value("")
                .conflicts_with("wasm_file")
                .help("Options to pass to the build command, for example --build-options='--lint-dir='"),
        )
        .arg(
            Arg::new("out")
                .long("out")
                .short('o')
                .value_parser(clap::value_parser!(PathBuf))
                .help("Where to write the migration, instead of asking. Defaults to `src/migration_<hash>.rs` in the module"),
        )
        .arg(common_args::anonymous())
        .arg(common_args::yes())
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(ArgAction::SetTrue)
                .help("Ignore spacetime.json configuration"),
        )
}

pub async fn exec(config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    match args.subcommand() {
        Some(("generate", args)) => exec_generate(config, args).await,
        Some((cmd, _)) => anyhow::bail!("Invalid subcommand: migrate {cmd}"),
        None => unreachable!("`migrate` requires a subcommand"),
    }
}

async fn exec_generate(mut config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let no_config = args.get_flag("no_config");
    let force = args.get_flag("force");
    let anon_identity = args.get_flag("anon_identity");

    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_database_arg(
        args.get_one::<String>("database").map(|s| s.as_str()),
        config_targets.as_deref(),
        GENERATE_USAGE,
    )?;
    let server = args
        .get_one::<String>("server")
        .map(|s| s.as_str())
        .or(resolved.server.as_deref());

    let module_path = module_path(args, &resolved.database, no_config)?;
    if !module_path.join("Cargo.toml").is_file() {
        anyhow::bail!(
            "`{}` is not a Rust module. Generating migrations is only supported for Rust modules. \
             If this is not the right module, pass --module-path.",
            module_path.display()
        );
    }

    // The schema the database is running now...
    let api = ClientApi::new(crate::api::Connection {
        host: config.get_host_url(server)?,
        auth_header: get_auth_header(&mut config, anon_identity, server, !force).await?,
        database_identity: database_identity(&config, &resolved.database, server).await?,
        database: resolved.database.clone(),
    });
    let old: ModuleDef = api
        .module_def()
        .await
        .with_context(|| format!("could not get the schema of database `{}`", resolved.database))?
        .try_into()
        .context("the database's schema is invalid")?;

    // ...and the schema of the module that will replace it.
    let wasm_path = match args.get_one::<PathBuf>("wasm_file") {
        Some(path) => {
            println!("Skipping build. Instead we are inspecting {}", path.display());
            path.clone()
        }
        None => {
            let build_options = args.get_one::<String>("build_options").unwrap();
            build::exec_with_argstring(&module_path, build_options, false, None)
                .await?
                .0
        }
    };
    let new = generate::extract_descriptions(&wasm_path).context("could not extract the module's schema")?;

    let dropped = DroppedSchema::new(&old, &new).with_context(|| {
        format!(
            "the module's schema can't be migrated from the schema of database `{}`",
            resolved.database
        )
    })?;
    if dropped.tables.is_empty() {
        println!(
            "Note: the module's schema doesn't remove or incompatibly change any tables, \
             so the migration won't see any dropped tables."
        );
    } else {
        let names: Vec<&str> = dropped.tables.iter().map(|t| &*t.accessor_name).collect();
        println!("Tables to migrate: {}", names.join(", "));
    }
    if !dropped.lost_sequences.is_empty() {
        eprintln!(
            "Warning: the new schema has no sequence with the same name as these sequences of changed tables, \
             so their state will be lost and the new tables' sequences will start over:"
        );
        for lost in &dropped.lost_sequences {
            eprintln!("  - `{}` ({}.{})", lost.sequence, lost.table, lost.column);
        }
        eprintln!(
            "If the migration copies rows with values generated by these sequences, \
             new rows may be given values that are already in use."
        );
    }
    let file = Rust.generate_migration(&dropped)?;

    let default_path = module_path.join("src").join(&file.filename);
    let Some(out_path) = choose_out_path(args, &default_path, force)? else {
        println!("Aborting");
        return Ok(());
    };
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out_path, file.code).with_context(|| format!("could not write {}", out_path.display()))?;

    let mod_name = out_path.file_stem().unwrap_or_default().to_string_lossy();
    println!("Wrote the migration to {}.", out_path.display());
    println!(
        "Add `mod {mod_name};` to your module, fill in its `migrate` function, \
         then publish the module to run it."
    );
    Ok(())
}

/// The module to build: `--module-path`, else the configured module path of `database`,
/// else the default `spacetime publish` uses.
fn module_path(args: &ArgMatches, database: &str, no_config: bool) -> anyhow::Result<PathBuf> {
    if let Some(path) = args.get_one::<PathBuf>("module_path") {
        return Ok(path.clone());
    }
    if !no_config && let Some(loaded) = find_and_load_with_env(None)? {
        let configured = loaded
            .config
            .collect_all_targets_with_inheritance()
            .into_iter()
            .filter(|target| target.fields.get("database").and_then(|v| v.as_str()) == Some(database))
            .find_map(|target| Some(target.fields.get("module-path")?.as_str()?.to_owned()));
        if let Some(path) = configured {
            return Ok(loaded.config_dir.join(path));
        }
    }
    Ok(default_publish_module_path(&std::env::current_dir()?))
}

/// Where to write the migration: `--out`, else what the user enters, defaulting to `default_path`.
///
/// Returns `None` if the user declines to overwrite an existing file.
fn choose_out_path(args: &ArgMatches, default_path: &Path, force: bool) -> anyhow::Result<Option<PathBuf>> {
    let path = if let Some(path) = args.get_one::<PathBuf>("out") {
        path.clone()
    } else if force || !std::io::stdin().is_terminal() {
        default_path.to_path_buf()
    } else {
        let default = display_relative(default_path);
        let entered: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Where should the migration be written?")
            .default(default)
            .interact_text()?;
        PathBuf::from(entered)
    };

    if path.exists() {
        // An existing migration may have been filled in already, so never replace one without asking.
        if force || !std::io::stdin().is_terminal() {
            anyhow::bail!(
                "{} already exists. Delete it or pass --out to write the migration elsewhere.",
                path.display()
            );
        }
        if !y_or_n(false, &format!("{} already exists. Overwrite it?", path.display()))? {
            return Ok(None);
        }
    }
    Ok(Some(path))
}

/// `path`, relative to the current directory if it's inside it.
fn display_relative(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}
