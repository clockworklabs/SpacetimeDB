use std::borrow::Cow;
use std::io::{self, Write};

use crate::common_args;
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_arg};
use crate::util::{add_auth_header_opt, database_identity, get_auth_header};
use clap::{Arg, ArgAction, ArgMatches};
use futures::{AsyncBufReadExt, TryStreamExt};
use is_terminal::IsTerminal;
use termcolor::{Color, ColorSpec, WriteColor};
use tokio::io::AsyncWriteExt;

pub fn cli() -> clap::Command {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(false)
                .help("The name or identity of the database to print logs from"),
        )
        .arg(
            common_args::server()
                .help("The nickname, host name or URL of the server hosting the database"),
        )
        .arg(
            Arg::new("num_lines")
                .long("num-lines")
                .short('n')
                .value_parser(clap::value_parser!(u32))
                .help("The number of lines to print from the start of the log of this database")
                .long_help("The number of lines to print from the start of the log of this database. If no num lines is provided, all lines will be returned."),
        )
        .arg(
            Arg::new("follow")
                .long("follow")
                .short('f')
                .required(false)
                .action(ArgAction::SetTrue)
                .help("A flag indicating whether or not to follow the logs")
                .long_help("A flag that causes logs to not stop when end of the log file is reached, but rather to wait for additional data to be appended to the input."),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .default_value("text")
                .required(false)
                .value_parser(clap::value_parser!(Format))
                .help("Output format for the logs")
        )
        .arg(
            Arg::new("level")
                .long("level")
                .short('l')
                .value_parser(clap::value_parser!(LogLevel))
                .help("Minimum log level to display")
                .long_help(
                    "Filter logs by severity level. Only messages at the specified level or higher \
                     will be shown. Levels from least to most severe: trace, debug, info, warn, error, panic.",
                ),
        )
        .arg(
            Arg::new("level_exact")
                .long("level-exact")
                .requires("level")
                .action(ArgAction::SetTrue)
                .help("Show only logs at exactly the specified level")
                .long_help(
                    "When combined with --level, show only logs at exactly the specified level \
                     instead of that level and above.",
                ),
        )
        .arg(common_args::yes())
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(ArgAction::SetTrue)
                .help("Ignore spacetime.json configuration"),
        )
        .after_help("Run `spacetime help logs` for more detailed information.\n")
}

#[derive(Clone, Copy, serde::Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic,
}

impl LogLevel {
    /// Returns a numeric severity value. Higher means more severe.
    fn severity(self) -> u8 {
        match self {
            LogLevel::Trace => 0,
            LogLevel::Debug => 1,
            LogLevel::Info => 2,
            LogLevel::Warn => 3,
            LogLevel::Error => 4,
            LogLevel::Panic => 5,
        }
    }
}

impl clap::ValueEnum for LogLevel {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Trace,
            Self::Debug,
            Self::Info,
            Self::Warn,
            Self::Error,
            Self::Panic,
        ]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Trace => Some(clap::builder::PossibleValue::new("trace")),
            Self::Debug => Some(clap::builder::PossibleValue::new("debug")),
            Self::Info => Some(clap::builder::PossibleValue::new("info")),
            Self::Warn => Some(clap::builder::PossibleValue::new("warn")),
            Self::Error => Some(clap::builder::PossibleValue::new("error")),
            Self::Panic => Some(clap::builder::PossibleValue::new("panic")),
        }
    }
}

/// Sentinel value used for injected system logs.
///
/// Keep this in sync with the constants in `spacetimedb_core::database_logger::Record`.
const SENTINEL: &str = "__spacetimedb__";

/// Keep this in sync with `spacetimedb_core::database_logger::Record`.
#[serde_with::serde_as]
#[derive(serde::Deserialize)]
struct Record<'a> {
    #[serde_as(as = "Option<serde_with::TimestampMicroSeconds>")]
    ts: Option<chrono::DateTime<chrono::Utc>>, // TODO: remove Option once 0.9 has been out for a while
    level: LogLevel,
    #[serde(borrow)]
    #[allow(unused)] // TODO: format this somehow
    target: Option<Cow<'a, str>>,
    #[serde(borrow)]
    filename: Option<Cow<'a, str>>,
    line_number: Option<u32>,
    #[serde(borrow)]
    function: Option<Cow<'a, str>>,
    #[serde(borrow)]
    message: Cow<'a, str>,
    trace: Option<Vec<BacktraceFrame<'a>>>,
}

#[derive(serde::Deserialize)]
pub struct BacktraceFrame<'a> {
    #[serde(borrow)]
    pub module_name: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub func_name: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub file: Option<Cow<'a, str>>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    #[serde(default)]
    pub symbols: Vec<BacktraceFrameSymbol<'a>>,
    #[serde(default)]
    pub kind: BacktraceFrameKind,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BacktraceFrameKind {
    #[default]
    Wasm,
    Js,
}

#[derive(serde::Deserialize)]
pub struct BacktraceFrameSymbol<'a> {
    #[serde(borrow)]
    pub name: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub file: Option<Cow<'a, str>>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(serde::Serialize)]
struct LogsParams {
    num_lines: Option<u32>,
    follow: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Text,
    Json,
}

impl clap::ValueEnum for Format {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Text, Self::Json]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Text => Some(clap::builder::PossibleValue::new("text").aliases(["default", "txt"])),
            Self::Json => Some(clap::builder::PossibleValue::new("json")),
        }
    }
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server_from_cli = args.get_one::<String>("server").map(|s| s.as_ref());
    let no_config = args.get_flag("no_config");
    let database_arg = args.get_one::<String>("database").map(|s| s.as_str());
    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_database_arg(
        database_arg,
        config_targets.as_deref(),
        "spacetime logs [database] [--no-config]",
    )?;
    let server = server_from_cli.or(resolved.server.as_deref());
    let force = args.get_flag("force");
    let mut num_lines = args.get_one::<u32>("num_lines").copied();
    let follow = args.get_flag("follow");
    let format = *args.get_one::<Format>("format").unwrap();
    let min_level = args.get_one::<LogLevel>("level").copied();
    let level_exact = args.get_flag("level_exact");

    let auth_header = get_auth_header(&mut config, false, server, !force).await?;

    let database_identity = database_identity(&config, &resolved.database, server).await?;

    if follow && num_lines.is_none() {
        // We typically don't want logs from the very beginning if we're also following.
        num_lines = Some(10);
    }
    let query_params = LogsParams { num_lines, follow };

    let host_url = config.get_host_url(server)?;

    let builder = reqwest::Client::new().get(format!("{host_url}/v1/database/{database_identity}/logs"));
    let builder = add_auth_header_opt(builder, &auth_header);
    let mut res = builder.query(&query_params).send().await?;
    let status = res.status();

    if status.is_client_error() || status.is_server_error() {
        let err = res.text().await?;
        anyhow::bail!(err)
    }

    if format == Format::Json {
        let mut stdout = tokio::io::stdout();
        if min_level.is_none() {
            // Fast path: no filtering, stream raw bytes.
            while let Some(chunk) = res.chunk().await? {
                stdout.write_all(&chunk).await?;
            }
        } else {
            // Parse each line to apply level filtering, then re-emit as JSON.
            let mut rdr = res.bytes_stream().map_err(io::Error::other).into_async_read();
            let mut line = String::new();
            while rdr.read_line(&mut line).await? != 0 {
                let record = serde_json::from_str::<Record<'_>>(&line)?;
                if should_display(record.level, min_level, level_exact) {
                    stdout.write_all(line.as_bytes()).await?;
                }
                line.clear();
            }
        }
        return Ok(());
    }

    let term_color = if std::io::stdout().is_terminal() {
        termcolor::ColorChoice::Auto
    } else {
        termcolor::ColorChoice::Never
    };
    let out = termcolor::StandardStream::stdout(term_color);
    let mut out = out.lock();

    let mut rdr = res.bytes_stream().map_err(io::Error::other).into_async_read();
    let mut line = String::new();
    while rdr.read_line(&mut line).await? != 0 {
        let record = serde_json::from_str::<Record<'_>>(&line)?;

        // Apply log level filtering.
        if !should_display(record.level, min_level, level_exact) {
            line.clear();
            continue;
        }

        if let Some(ts) = record.ts {
            out.set_color(ColorSpec::new().set_dimmed(true))?;
            write!(out, "{ts:?} ")?;
        }
        let mut color = ColorSpec::new();
        let level = match record.level {
            LogLevel::Error => {
                color.set_fg(Some(Color::Red));
                "ERROR"
            }
            LogLevel::Warn => {
                color.set_fg(Some(Color::Yellow));
                "WARN"
            }
            LogLevel::Info => {
                color.set_fg(Some(Color::Blue));
                "INFO"
            }
            LogLevel::Debug => {
                color.set_dimmed(true).set_bold(true);
                "DEBUG"
            }
            LogLevel::Trace => {
                color.set_dimmed(true);
                "TRACE"
            }
            LogLevel::Panic => {
                color.set_fg(Some(Color::Red)).set_bold(true).set_intense(true);
                "PANIC"
            }
        };
        out.set_color(&color)?;
        write!(out, "{level:>5}: ")?;
        out.reset()?;
        let mut need_space_before_filename = false;
        let mut need_colon_sep = false;
        let dimmed = ColorSpec::new().set_dimmed(true).clone();
        if let Some(function) = record.function
            && function != SENTINEL
        {
            out.set_color(&dimmed)?;
            write!(out, "{function}")?;
            out.reset()?;
            need_space_before_filename = true;
            need_colon_sep = true;
        }
        if let Some(filename) = record.filename
            && filename != SENTINEL
        {
            out.set_color(&dimmed)?;
            if need_space_before_filename {
                write!(out, " ")?;
            }
            write!(out, "{filename}")?;
            if let Some(line) = record.line_number {
                write!(out, ":{line}")?;
            }
            out.reset()?;
            need_colon_sep = true;
        }
        if need_colon_sep {
            write!(out, ": ")?;
        }
        writeln!(out, "{}", record.message)?;
        if let Some(trace) = &record.trace {
            writeln!(out, "backtrace:")?;
            fmt_backtrace(&mut out, trace)?;
        }

        line.clear();
    }

    Ok(())
}

// based on fmt::Display impl for wasmtime::WasmBacktrace
// modified to print in color and to skip irrelevant frames
fn fmt_backtrace<W: WriteColor>(out: &mut W, trace: &[BacktraceFrame<'_>]) -> anyhow::Result<()> {
    for (frame_i, frame) in trace.iter().enumerate() {
        let func_name = frame.func_name.as_deref().unwrap_or("<unknown>");
        let module_name = frame.module_name.as_deref();
        write!(out, "  {:>3}: ", frame_i)?;

        let write_func_name = |out: &mut W, name: &str| {
            let (name, suffix) = match frame.kind {
                BacktraceFrameKind::Js => (name, None),
                BacktraceFrameKind::Wasm => {
                    let has_hash_suffix = name.len() > 19
                        && &name[name.len() - 19..name.len() - 16] == "::h"
                        && name[name.len() - 16..].chars().all(|x| x.is_ascii_hexdigit());
                    let (name_no_suffix, suffix) = has_hash_suffix.then(|| name.split_at(name.len() - 19)).unzip();
                    (name_no_suffix.unwrap_or(name), suffix)
                }
            };
            out.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
            write!(out, "{name}")?;
            if let Some(suffix) = suffix {
                out.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_dimmed(true))?;
                write!(out, "{suffix}")?;
            }
            out.reset()
        };
        if frame.symbols.is_empty() {
            if let Some(module_name) = module_name {
                write!(out, "{module_name}!")?;
            }
            write_func_name(out, func_name)?;
            writeln!(out)?;
        } else {
            for (i, symbol) in frame.symbols.iter().enumerate() {
                if i > 0 {
                    write!(out, "       ")?;
                } else {
                    // ...
                }
                let symbol_name = match &symbol.name {
                    Some(name) => name,
                    None if i == 0 => func_name,
                    None => "<inlined function>",
                };
                write_func_name(out, symbol_name)?;
                if let Some(file) = &symbol.file {
                    writeln!(out)?;
                    write!(out, "         at {}", file)?;
                    if let Some(line) = symbol.line {
                        write!(out, ":{}", line)?;
                        if let Some(col) = symbol.column {
                            write!(out, ":{}", col)?;
                        }
                    }
                }
                writeln!(out)?;
            }
        }
    }
    Ok(())
}

/// Returns true if the record should be displayed given the filter settings.
fn should_display(record_level: LogLevel, min_level: Option<LogLevel>, level_exact: bool) -> bool {
    match min_level {
        None => true,
        Some(min) => {
            if level_exact {
                record_level.severity() == min.severity()
            } else {
                record_level.severity() >= min.severity()
            }
        }
    }
}
