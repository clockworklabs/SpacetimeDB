use crate::common_args;
use crate::config::Config;
use crate::edit_distance::{edit_distance, find_best_match_for_name};
use crate::util;
use crate::util::{add_auth_header_opt, database_identity, get_auth_header_only};
use anyhow::{bail, Context, Error};
use clap::{Arg, ArgMatches};
use itertools::Either;
use serde_json::Value;
use spacetimedb_lib::de::serde::deserialize_from;
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, Typespace};
use spacetimedb_lib::{Address, ProductTypeElement};
use std::fmt::Write;
use std::iter;

pub fn cli() -> clap::Command {
    clap::Command::new("call")
        .about("Invokes a reducer function in a database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The database domain or identity to use to invoke the call"),
        )
        .arg(
            Arg::new("reducer_name")
                .required(true)
                .help("The name of the reducer to call"),
        )
        .arg(Arg::new("arguments").help("arguments formatted as JSON").num_args(1..))
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(
            common_args::identity()
                .conflicts_with("anon_identity")
                .help("The identity to use for the call"),
        )
        .arg(common_args::anonymous())
        .after_help("Run `spacetime help call` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), Error> {
    let database = args.get_one::<String>("database").unwrap();
    let reducer_name = args.get_one::<String>("reducer_name").unwrap();
    let arguments = args.get_many::<String>("arguments");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());

    let identity = args.get_one::<String>("identity");
    let anon_identity = args.get_flag("anon_identity");

    let identity = database_identity(&config, database, server).await?;

    let builder = reqwest::Client::new().post(format!(
        "{}/database/call/{}/{}",
        config.get_host_url(server)?,
        identity.clone(),
        reducer_name
    ));
    let auth_header = get_auth_header_only(&mut config, anon_identity, identity, server).await?;
    let builder = add_auth_header_opt(builder, &auth_header);
    let describe_reducer = util::describe_reducer(
        &mut config,
        identity,
        server.map(|x| x.to_string()),
        reducer_name.clone(),
        anon_identity,
        identity.cloned(),
    )
    .await?;

    // String quote any arguments that should be quoted
    let arguments = arguments
        .unwrap_or_default()
        .zip(describe_reducer.schema.elements.iter())
        .map(|(argument, element)| match &element.algebraic_type {
            AlgebraicType::String if !argument.starts_with('\"') || !argument.ends_with('\"') => {
                format!("\"{}\"", argument)
            }
            _ => argument.to_string(),
        })
        .collect::<Vec<_>>();

    let arg_json = format!("[{}]", arguments.join(", "));
    let res = builder.body(arg_json.to_owned()).send().await?;

    if let Err(e) = res.error_for_status_ref() {
        let Ok(response_text) = res.text().await else {
            // Cannot give a better error than this if we don't know what the problem is.
            bail!(e);
        };

        let error = Err(e).context(format!("Response text: {}", response_text));

        let error_msg = if response_text.starts_with("no such reducer") {
            no_such_reducer(config, &identity, database, &auth_header, reducer_name, server).await
        } else if response_text.starts_with("invalid arguments") {
            invalid_arguments(
                config,
                &identity,
                database,
                &auth_header,
                reducer_name,
                &response_text,
                server,
            )
            .await
        } else {
            return error;
        };

        return error.context(error_msg);
    }

    Ok(())
}

/// Returns an error message for when `reducer` is called with wrong arguments.
async fn invalid_arguments(
    config: Config,
    identity: &Identity,
    db: &str,
    auth_header: &Option<String>,
    reducer: &str,
    text: &str,
    server: Option<&str>,
) -> String {
    let mut error = format!(
        "Invalid arguments provided for reducer `{}` for database `{}` resolving to identity `{}`.",
        reducer, db, identity
    );

    if let Some((actual, expected)) = find_actual_expected(text).filter(|(a, e)| a != e) {
        write!(
            error,
            "\n\n{} parameters were expected, but {} were provided.",
            expected, actual
        )
        .unwrap();
    }

    if let Some(sig) = schema_json(config, identity, auth_header, true, server)
        .await
        .and_then(|schema| reducer_signature(schema, reducer))
    {
        write!(error, "\n\nThe reducer has the following signature:\n\t{}", sig).unwrap();
    }

    error
}

/// Parse actual/expected parameter numbers from the invalid args response text.
fn find_actual_expected(text: &str) -> Option<(usize, usize)> {
    let (_, x) = split_at_first_substring(text, "invalid length")?;
    let (x, y) = split_at_first_substring(x, "args for test with")?;
    let (x, _) = split_at_first_substring(x, ",")?;
    let (y, _) = split_at_first_substring(y, "elements")?;
    let actual: usize = x.trim().parse().ok()?;
    let expected: usize = y.trim().parse().ok()?;
    Some((actual, expected))
}

/// Returns a tuple with
/// - everything after the first `substring`
/// - and anything before it.
fn split_at_first_substring<'t>(text: &'t str, substring: &str) -> Option<(&'t str, &'t str)> {
    text.find(substring)
        .map(|pos| (&text[..pos], &text[pos + substring.len()..]))
}

/// Provided the `schema_json` for the database,
/// returns the signature for a reducer with `reducer_name`.
fn reducer_signature(schema_json: Value, reducer_name: &str) -> Option<String> {
    let typespace = typespace(&schema_json)?;

    // Fetch the matching reducer.
    let elements = find_of_type_in_schema(&schema_json, "reducer")
        .find(|(name, _)| *name == reducer_name)?
        .1
        .get("schema")?
        .get("elements")?;
    let params = deserialize_from::<Vec<ProductTypeElement>, _>(elements).ok()?;

    // Print the arguments to `args`.
    let mut args = String::new();
    fn ctx(typespace: &Typespace, r: AlgebraicTypeRef) -> String {
        let ty = &typespace[r];
        let mut ty_str = String::new();
        write_type::write_type(&|r| ctx(typespace, r), &mut ty_str, ty).unwrap();
        ty_str
    }
    write_type::write_arglist_no_delimiters(&|r| ctx(&typespace, r), &mut args, &params, None).unwrap();
    let args = args.trim().trim_end_matches(',').replace('\n', " ");

    // Print the full signature to `reducer_fmt`.
    let mut reducer_fmt = String::new();
    write!(&mut reducer_fmt, "{}({})", reducer_name, args).unwrap();
    Some(reducer_fmt)
}

/// Returns an error message for when `reducer` does not exist in `db`.
async fn no_such_reducer(
    config: Config,
    identity: &Identity,
    db: &str,
    auth_header: &Option<String>,
    reducer: &str,
    server: Option<&str>,
) -> String {
    let mut error = format!(
        "No such reducer `{}` for database `{}` resolving to identity `{}`.",
        reducer, db, identity
    );

    if let Some(schema) = schema_json(config, identity, auth_header, false, server).await {
        add_reducer_ctx_to_err(&mut error, schema, reducer);
    }

    error
}

const REDUCER_PRINT_LIMIT: usize = 10;

/// Provided the schema for the database,
/// decorate `error` with more helpful info about reducers.
fn add_reducer_ctx_to_err(error: &mut String, schema_json: Value, reducer_name: &str) {
    let mut reducers = find_of_type_in_schema(&schema_json, "reducer")
        .map(|kv| kv.0)
        .collect::<Vec<_>>();

    // Hide pseudo-reducers (assume that any `__XXX__` are such); they shouldn't be callable.
    reducers.retain(|&c| !(c.starts_with("__") && c.ends_with("__")));

    if let Some(best) = find_best_match_for_name(&reducers, reducer_name, None) {
        write!(error, "\n\nA reducer with a similar name exists: `{}`", best).unwrap();
    } else if reducers.is_empty() {
        write!(error, "\n\nThe database has no reducers.").unwrap();
    } else {
        // Sort reducers by relevance.
        reducers.sort_by_key(|candidate| edit_distance(reducer_name, candidate, usize::MAX));

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

/// Fetch the schema as JSON for the database at `identity`.
///
/// The value of `expand` determines how detailed information to fetch.
async fn schema_json(
    config: Config,
    identity: &Identity,
    auth_header: &Option<String>,
    expand: bool,
    server: Option<&str>,
) -> Option<Value> {
    let builder = reqwest::Client::new().get(format!(
        "{}/database/schema/{}",
        config.get_host_url(server).ok()?,
        identity
    ));
    let builder = add_auth_header_opt(builder, auth_header);

    builder
        .query(&[("expand", expand)])
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
fn find_of_type_in_schema<'v, 't: 'v>(
    value: &'v serde_json::Value,
    ty: &'t str,
) -> impl Iterator<Item = (&'v str, &'v Value)> {
    let Some(entities) = value
        .as_object()
        .and_then(|o| o.get("entities"))
        .and_then(|e| e.as_object())
    else {
        return Either::Left(iter::empty());
    };

    let iter = entities
        .into_iter()
        .filter(move |(_, value)| {
            let Some(obj) = value.as_object() else {
                return false;
            };
            obj.get("type").filter(|x| x.as_str() == Some(ty)).is_some()
        })
        .map(|(key, value)| (key.as_str(), value));
    Either::Right(iter)
}

/// Returns the `Typespace` in the provided json schema.
fn typespace(value: &serde_json::Value) -> Option<Typespace> {
    let types = value.as_object()?.get("typespace")?;
    deserialize_from(types).map(Typespace::new).ok()
}

// this is an old version of code in generate::rust that got
// refactored, but reducer_signature() was using it
// TODO: port reducer_signature() to use AlgebraicTypeUse et al, somehow.
mod write_type {
    use super::*;
    use convert_case::{Case, Casing};
    use spacetimedb_lib::sats::ArrayType;
    use spacetimedb_lib::ProductType;
    use std::fmt;
    use std::ops::Deref;

    pub fn write_type<W: fmt::Write>(
        ctx: &impl Fn(AlgebraicTypeRef) -> String,
        out: &mut W,
        ty: &AlgebraicType,
    ) -> fmt::Result {
        match ty {
            p if p.is_identity() => write!(out, "Identity")?,
            p if p.is_address() => write!(out, "Address")?,
            p if p.is_schedule_at() => write!(out, "ScheduleAt")?,
            AlgebraicType::Sum(sum_type) => {
                if let Some(inner_ty) = sum_type.as_option() {
                    write!(out, "Option<")?;
                    write_type(ctx, out, inner_ty)?;
                    write!(out, ">")?;
                } else {
                    write!(out, "enum ")?;
                    print_comma_sep_braced(out, &sum_type.variants, |out: &mut W, elem: &_| {
                        if let Some(name) = &elem.name {
                            write!(out, "{name}: ")?;
                        }
                        write_type(ctx, out, &elem.algebraic_type)
                    })?;
                }
            }
            AlgebraicType::Product(ProductType { elements }) => {
                print_comma_sep_braced(out, elements, |out: &mut W, elem: &ProductTypeElement| {
                    if let Some(name) = &elem.name {
                        write!(out, "{name}: ")?;
                    }
                    write_type(ctx, out, &elem.algebraic_type)
                })?;
            }
            AlgebraicType::Bool => write!(out, "bool")?,
            AlgebraicType::I8 => write!(out, "i8")?,
            AlgebraicType::U8 => write!(out, "u8")?,
            AlgebraicType::I16 => write!(out, "i16")?,
            AlgebraicType::U16 => write!(out, "u16")?,
            AlgebraicType::I32 => write!(out, "i32")?,
            AlgebraicType::U32 => write!(out, "u32")?,
            AlgebraicType::I64 => write!(out, "i64")?,
            AlgebraicType::U64 => write!(out, "u64")?,
            AlgebraicType::I128 => write!(out, "i128")?,
            AlgebraicType::U128 => write!(out, "u128")?,
            AlgebraicType::I256 => write!(out, "i256")?,
            AlgebraicType::U256 => write!(out, "u256")?,
            AlgebraicType::F32 => write!(out, "f32")?,
            AlgebraicType::F64 => write!(out, "f64")?,
            AlgebraicType::String => write!(out, "String")?,
            AlgebraicType::Array(ArrayType { elem_ty }) => {
                write!(out, "Vec<")?;
                write_type(ctx, out, elem_ty)?;
                write!(out, ">")?;
            }
            AlgebraicType::Ref(r) => {
                write!(out, "{}", ctx(*r))?;
            }
        }
        Ok(())
    }

    fn print_comma_sep_braced<W: fmt::Write, T>(
        out: &mut W,
        elems: &[T],
        on: impl Fn(&mut W, &T) -> fmt::Result,
    ) -> fmt::Result {
        write!(out, "{{")?;

        let mut iter = elems.iter();

        // First factor.
        if let Some(elem) = iter.next() {
            write!(out, " ")?;
            on(out, elem)?;
        }
        // Other factors.
        for elem in iter {
            write!(out, ", ")?;
            on(out, elem)?;
        }

        if !elems.is_empty() {
            write!(out, " ")?;
        }

        write!(out, "}}")?;

        Ok(())
    }

    pub fn write_arglist_no_delimiters(
        ctx: &impl Fn(AlgebraicTypeRef) -> String,
        out: &mut impl Write,
        elements: &[ProductTypeElement],

        // Written before each line. Useful for `pub`.
        prefix: Option<&str>,
    ) -> fmt::Result {
        for elt in elements {
            if let Some(prefix) = prefix {
                write!(out, "{prefix} ")?;
            }

            let Some(name) = &elt.name else {
                panic!("Product type element has no name: {elt:?}");
            };
            let name = name.deref().to_case(Case::Snake);

            write!(out, "{name}: ")?;
            write_type(ctx, out, &elt.algebraic_type)?;
            writeln!(out, ",")?;
        }
        Ok(())
    }
}
