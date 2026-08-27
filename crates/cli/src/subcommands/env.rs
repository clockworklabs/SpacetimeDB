use std::io::Read;

use crate::api::{from_json_seed, ClientApi, SqlStmtResult};
use crate::common_args;
use crate::config::Config;
use crate::subcommands::sql::parse_req;
use crate::util::{ResponseExt, UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use is_terminal::IsTerminal;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{ProductValue, Typespace};

pub fn cli() -> Command {
    Command::new("env")
        .about(format!(
            "Manage environment variables for a database. {UNSTABLE_WARNING}"
        ))
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("set")
            .about("Create or update an environment variable")
            .arg(
                Arg::new("database")
                    .required(true)
                    .help("The name or identity of the database"),
            )
            .arg(Arg::new("key").required(true).help("The name of the variable"))
            .arg(
                Arg::new("value")
                    .help("The value of the variable. If omitted, the value is read from stdin without echo"),
            )
            .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
            .arg(common_args::anonymous())
            .arg(common_args::yes()),
        Command::new("get")
            .about("Print the value of an environment variable")
            .arg(
                Arg::new("database")
                    .required(true)
                    .help("The name or identity of the database"),
            )
            .arg(Arg::new("key").required(true).help("The name of the variable"))
            .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
            .arg(common_args::anonymous())
            .arg(common_args::yes()),
        Command::new("del")
            .about("Remove an environment variable")
            .arg(
                Arg::new("database")
                    .required(true)
                    .help("The name or identity of the database"),
            )
            .arg(Arg::new("key").required(true).help("The name of the variable"))
            .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
            .arg(common_args::anonymous())
            .arg(common_args::yes()),
        Command::new("list")
            .about("List the names of all environment variables")
            .arg(
                Arg::new("database")
                    .required(true)
                    .help("The name or identity of the database"),
            )
            .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
            .arg(common_args::anonymous())
            .arg(common_args::yes()),
    ]
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    match cmd {
        "set" => exec_set(config, subcommand_args).await,
        "get" => exec_get(config, subcommand_args).await,
        "del" => exec_del(config, subcommand_args).await,
        "list" => exec_list(config, subcommand_args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

/// Validate an env var key, mirroring the server-side rules.
///
/// This also guarantees the key is safe to embed in a SQL statement.
fn validate_key(key: &str) -> anyhow::Result<()> {
    let mut bytes = key.bytes();
    let valid = key.len() <= 256
        && bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !valid {
        anyhow::bail!(
            "invalid env var key `{key}`: keys must match [A-Za-z_][A-Za-z0-9_]* and be at most 256 bytes"
        );
    }
    Ok(())
}

/// Read the value from the terminal without echo, or from piped stdin.
fn read_value_from_input() -> anyhow::Result<String> {
    if std::io::stdin().is_terminal() {
        Ok(dialoguer::Password::new()
            .with_prompt("Value")
            .allow_empty_password(true)
            .interact()?)
    } else {
        let mut value = String::new();
        std::io::stdin().read_to_string(&mut value)?;
        // Strip a single trailing newline from piped input.
        let value = value
            .strip_suffix('\n')
            .map(|v| v.strip_suffix('\r').unwrap_or(v))
            .unwrap_or(&value);
        Ok(value.to_string())
    }
}

async fn api_for(config: Config, args: &ArgMatches) -> anyhow::Result<ClientApi> {
    let database = args.get_one::<String>("database").unwrap();
    let con = parse_req(config, args, database, None).await?;
    Ok(ClientApi::new(con))
}

/// Execute `sql` and return the resulting rows.
async fn run_sql_returning_rows(api: &ClientApi, sql: String) -> anyhow::Result<Vec<ProductValue>> {
    let json = api
        .sql()
        .body(sql)
        .send()
        .await?
        .ensure_content_type("application/json")
        .await?
        .text()
        .await?;
    let stmt_results: Vec<SqlStmtResult> = serde_json::from_str(&json).context("malformed sql response")?;
    let mut rows = Vec::new();
    for stmt_result in &stmt_results {
        let ty = Typespace::EMPTY.with_type(&stmt_result.schema);
        for row in &stmt_result.rows {
            rows.push(from_json_seed(row.get(), SeedWrapper(ty))?);
        }
    }
    Ok(rows)
}

async fn exec_set(config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let key = args.get_one::<String>("key").unwrap();
    validate_key(key)?;
    let value = match args.get_one::<String>("value") {
        Some(value) => value.clone(),
        None => read_value_from_input()?,
    };
    let api = api_for(config, args).await?;
    // Single quotes are escaped by doubling them.
    let value = value.replace('\'', "''");
    run_sql_returning_rows(&api, format!("SET env.{key} = '{value}'")).await?;
    println!("Set env var `{key}`");
    Ok(())
}

async fn exec_get(config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let key = args.get_one::<String>("key").unwrap();
    validate_key(key)?;
    let api = api_for(config, args).await?;
    let rows = run_sql_returning_rows(&api, format!("SELECT value FROM st_env WHERE key = '{key}'")).await?;
    match rows.first().and_then(|row| row.elements.first()) {
        Some(value) => {
            let value = value.as_string().context("expected a string value")?;
            println!("{value}");
            Ok(())
        }
        None => anyhow::bail!("env var `{key}` is not set"),
    }
}

async fn exec_del(config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let key = args.get_one::<String>("key").unwrap();
    validate_key(key)?;
    let api = api_for(config, args).await?;
    run_sql_returning_rows(&api, format!("DELETE env.{key}")).await?;
    println!("Deleted env var `{key}`");
    Ok(())
}

async fn exec_list(config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let api = api_for(config, args).await?;
    let rows = run_sql_returning_rows(&api, "SELECT key FROM st_env".to_string()).await?;
    for row in &rows {
        if let Some(key) = row.elements.first().and_then(|key| key.as_string()) {
            println!("{key}");
        }
    }
    Ok(())
}
