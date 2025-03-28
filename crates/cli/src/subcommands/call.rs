use crate::api::ClientApi;
use crate::common_args;
use crate::config::Config;
use crate::edit_distance::{edit_distance, find_best_match_for_name};
use crate::util::UNSTABLE_WARNING;
use anyhow::{bail, Context, Error};
use clap::{Arg, ArgMatches};
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb::Identity;
use spacetimedb_lib::sats::{self, AlgebraicType, Typespace};
use spacetimedb_lib::ProductTypeElement;
use spacetimedb_schema::def::{ModuleDef, ReducerDef};
use std::fmt::Write;

use super::sql::parse_req;

pub fn cli() -> clap::Command {
    clap::Command::new("call")
        .about(format!(
            "Invokes a reducer function in a database. {}",
            UNSTABLE_WARNING
        ))
        .arg(
            Arg::new("database")
                .required(true)
                .help("The database name or identity to use to invoke the call"),
        )
        .arg(
            Arg::new("reducer_name")
                .required(true)
                .help("The name of the reducer to call"),
        )
        .arg(Arg::new("arguments").help("arguments formatted as JSON").num_args(1..))
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::anonymous())
        .arg(common_args::yes())
        .after_help("Run `spacetime help call` for more detailed information.\n")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), Error> {
    eprintln!("{}\n", UNSTABLE_WARNING);
    let reducer_name = args.get_one::<String>("reducer_name").unwrap();
    let arguments = args.get_many::<String>("arguments");

    let conn = parse_req(config, args).await?;
    let api = ClientApi::new(conn);

    let database_identity = api.con.database_identity;
    let database = &api.con.database;

    let module_def: ModuleDef = api.module_def().await?.try_into()?;

    let reducer_def = module_def
        .reducer(&**reducer_name)
        .ok_or_else(|| anyhow::Error::msg(no_such_reducer(&database_identity, database, reducer_name, &module_def)))?;

    // String quote any arguments that should be quoted
    let arguments = arguments
        .unwrap_or_default()
        .zip(&*reducer_def.params.elements)
        .map(|(argument, element)| match &element.algebraic_type {
            AlgebraicType::String if !argument.starts_with('\"') || !argument.ends_with('\"') => {
                format!("\"{}\"", argument)
            }
            _ => argument.to_string(),
        });

    let arg_json = format!("[{}]", arguments.format(", "));
    let res = api.call(reducer_name, arg_json).await?;

    if let Err(e) = res.error_for_status_ref() {
        let Ok(response_text) = res.text().await else {
            // Cannot give a better error than this if we don't know what the problem is.
            bail!(e);
        };

        let error = Err(e).context(format!("Response text: {}", response_text));

        let error_msg = if response_text.starts_with("no such reducer") {
            no_such_reducer(&database_identity, database, reducer_name, &module_def)
        } else if response_text.starts_with("invalid arguments") {
            invalid_arguments(&database_identity, database, &response_text, &module_def, reducer_def)
        } else {
            return error;
        };

        return error.context(error_msg);
    }

    Ok(())
}

/// Returns an error message for when `reducer` is called with wrong arguments.
fn invalid_arguments(
    identity: &Identity,
    db: &str,
    text: &str,
    module_def: &ModuleDef,
    reducer_def: &ReducerDef,
) -> String {
    let mut error = format!(
        "Invalid arguments provided for reducer `{}` for database `{}` resolving to identity `{}`.",
        reducer_def.name, db, identity
    );

    if let Some((actual, expected)) = find_actual_expected(text).filter(|(a, e)| a != e) {
        write!(
            error,
            "\n\n{} parameters were expected, but {} were provided.",
            expected, actual
        )
        .unwrap();
    }

    write!(
        error,
        "\n\nThe reducer has the following signature:\n\t{}",
        ReducerSignature(module_def.typespace().with_type(reducer_def))
    )
    .unwrap();

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
struct ReducerSignature<'a>(sats::WithTypespace<'a, ReducerDef>);
impl std::fmt::Display for ReducerSignature<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reducer_def = self.0.ty();
        let typespace = self.0.typespace();

        write!(f, "{}(", reducer_def.name)?;

        // Print the arguments to `args`.
        let mut comma = false;
        for arg in &*reducer_def.params.elements {
            if comma {
                write!(f, ", ")?;
            }
            comma = true;
            if let Some(name) = arg.name() {
                write!(f, "{}: ", name.to_case(Case::Snake))?;
            }
            write_type::write_type(typespace, f, &arg.algebraic_type)?;
        }

        write!(f, ")")
    }
}

/// Returns an error message for when `reducer` does not exist in `db`.
fn no_such_reducer(database_identity: &Identity, db: &str, reducer: &str, module_def: &ModuleDef) -> String {
    let mut error = format!(
        "No such reducer `{}` for database `{}` resolving to identity `{}`.",
        reducer, db, database_identity
    );

    add_reducer_ctx_to_err(&mut error, module_def, reducer);

    error
}

const REDUCER_PRINT_LIMIT: usize = 10;

/// Provided the schema for the database,
/// decorate `error` with more helpful info about reducers.
fn add_reducer_ctx_to_err(error: &mut String, module_def: &ModuleDef, reducer_name: &str) {
    let mut reducers = module_def
        .reducers()
        .filter(|reducer| reducer.lifecycle.is_none())
        .map(|reducer| &*reducer.name)
        .collect::<Vec<_>>();

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

// this is an old version of code in generate::rust that got
// refactored, but reducer_signature() was using it
// TODO: port reducer_signature() to use AlgebraicTypeUse et al, somehow.
mod write_type {
    use super::*;
    use sats::ArrayType;
    use spacetimedb_lib::ProductType;
    use std::fmt;

    pub fn write_type<W: fmt::Write>(typespace: &Typespace, out: &mut W, ty: &AlgebraicType) -> fmt::Result {
        match ty {
            p if p.is_identity() => write!(out, "Identity")?,
            p if p.is_connection_id() => write!(out, "ConnectionId")?,
            p if p.is_schedule_at() => write!(out, "ScheduleAt")?,
            AlgebraicType::Sum(sum_type) => {
                if let Some(inner_ty) = sum_type.as_option() {
                    write!(out, "Option<")?;
                    write_type(typespace, out, inner_ty)?;
                    write!(out, ">")?;
                } else {
                    write!(out, "enum ")?;
                    print_comma_sep_braced(out, &sum_type.variants, |out: &mut W, elem: &_| {
                        if let Some(name) = &elem.name {
                            write!(out, "{name}: ")?;
                        }
                        write_type(typespace, out, &elem.algebraic_type)
                    })?;
                }
            }
            AlgebraicType::Product(ProductType { elements }) => {
                print_comma_sep_braced(out, elements, |out: &mut W, elem: &ProductTypeElement| {
                    if let Some(name) = &elem.name {
                        write!(out, "{name}: ")?;
                    }
                    write_type(typespace, out, &elem.algebraic_type)
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
                write_type(typespace, out, elem_ty)?;
                write!(out, ">")?;
            }
            AlgebraicType::Ref(r) => {
                write_type(typespace, out, &typespace[*r])?;
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
}
