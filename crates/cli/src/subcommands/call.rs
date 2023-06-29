use crate::config::Config;
use crate::edit_distance::{edit_distance, find_best_match_for_name};
use crate::util::{add_auth_header_opt, database_address, get_auth_header_only};
use anyhow::{Context, Error};
use clap::{Arg, ArgAction, ArgMatches};
use serde_json::Value;
use std::fmt::Write;

pub fn cli() -> clap::Command {
    clap::Command::new("call")
        .about("Invokes a reducer function in a database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The database domain or address to use to invoke the call"),
        )
        .arg(
            Arg::new("reducer_name")
                .required(true)
                .help("The name of the reducer to call"),
        )
        .arg(
            Arg::new("arguments")
                .help("arguments as a JSON array")
                .default_value("[]"),
        )
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .conflicts_with("anon_identity")
                .help("The identity to use for the call"),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("as_identity")
                .action(ArgAction::SetTrue)
                .help("If this flag is present, the call will be executed with no identity provided"),
        )
        .after_help("Run `spacetime help call` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), Error> {
    let database = args.get_one::<String>("database").unwrap();
    let reducer_name = args.get_one::<String>("reducer_name").unwrap();
    let arg_json = args.get_one::<String>("arguments").unwrap();

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let address = database_address(&config, database).await?;

    let builder = reqwest::Client::new().post(format!(
        "{}/database/call/{}/{}",
        config.get_host_url(),
        address,
        reducer_name
    ));
    let auth_header = get_auth_header_only(&mut config, anon_identity, as_identity).await;
    let builder = add_auth_header_opt(builder, &auth_header);

    let res = builder.body(arg_json.to_owned()).send().await?;

    if let Err(e) = res.error_for_status_ref() {
        let mut error = format!(
            "No such reducer `{}` for database `{}` resolving to address `{}`.",
            reducer_name, database, address
        );

        if let Some(schema) = schema_json(config, &address, &auth_header).await {
            add_reducer_ctx_to_err(&mut error, schema, reducer_name);
        }

        return Err(e)
            .context(format!("Response text: {}", res.text().await?))
            .context(error);
    }

    Ok(())
}

const REDUCER_PRINT_LIMIT: usize = 10;

/// Provided the schema for the database,
/// decorate `error` with more helpful info.
fn add_reducer_ctx_to_err(error: &mut String, schema_json: Value, reducer_name: &str) {
    let mut reducers = find_of_type_in_schema(&schema_json, "reducer");

    // Hide these pseudo-reducers; they shouldn't be callable.
    reducers.retain(|&c| !matches!(c, "__update__" | "__init__"));

    if let Some(best) = find_best_match_for_name(&reducers, &reducer_name, None) {
        write!(error, "\n\nA reducer with a similar name exists: `{}`", best).unwrap();
    } else if reducers.is_empty() {
        write!(error, "\n\nThe database has no reducers.").unwrap();
    } else {
        // Sort reducers by relevance.
        reducers.sort_by_key(|candidate| edit_distance(&reducer_name, &candidate, usize::MAX));

        // Don't spam the user with too many entries.
        let too_many_to_show = reducers.len() > REDUCER_PRINT_LIMIT;
        let diff = reducers.len().abs_diff(REDUCER_PRINT_LIMIT);
        reducers.truncate(REDUCER_PRINT_LIMIT);

        // List them.
        write!(error, "\n\nHere are some existing reducers:").unwrap();
        for candidate in reducers {
            write!(error, "\n- {}", candidate).unwrap();
        }

        // When some where not listed, note that are more.
        if too_many_to_show {
            let plural = if diff == 1 { "" } else { "s" };
            write!(error, "\n... ({} reducer{} not shown)", diff, plural).unwrap();
        }
    }
}

/// Fetch the schema as JSON for the database at `address`.
async fn schema_json(config: Config, address: &str, auth_header: &Option<String>) -> Option<Value> {
    let builder = reqwest::Client::new().get(format!("{}/database/schema/{}", config.get_host_url(), address));
    let builder = add_auth_header_opt(builder, &auth_header);

    builder
        .query(&[("expand", false)])
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()
}

/// Returns all the names of items in `value` that match `type`.
///
/// For example, `type` can be `"reducer"`.
fn find_of_type_in_schema<'v, 't: 'v>(value: &'v serde_json::Value, ty: &'t str) -> Vec<&'v str> {
    let Some(obj) = value.as_object() else { return Vec::new() };
    obj.into_iter()
        .filter(|(_, value)| {
            let Some(obj) = value.as_object() else { return false; };
            obj.get("type").filter(|x| x.as_str() == Some(ty)).is_some()
        })
        .map(|o| o.0.as_str())
        .collect()
}
