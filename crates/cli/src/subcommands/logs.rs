use std::borrow::Cow;
use std::io::{self, Write};

use crate::config::Config;
use crate::util::{add_auth_header_opt, database_address, get_auth_header};
use clap::{Arg, ArgAction, ArgMatches};
use futures::{AsyncBufReadExt, TryStreamExt};
use is_terminal::IsTerminal;
use termcolor::{Color, ColorSpec, WriteColor};

pub fn cli() -> clap::Command {
    clap::Command::new("logs")
        .about("Prints logs from a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database to print logs from"),
        )
        .arg(
            Arg::new("identity")
                .long("identity")
                .short('i')
                .help("The identity to use for printing logs from this database"),
        )
        .arg(
            Arg::new("num_lines")
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
        .after_help("Run `spacetime help logs` for more detailed information.\n")
}

#[derive(serde::Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic,
}

#[derive(serde::Deserialize)]
struct Record<'a> {
    level: LogLevel,
    #[serde(borrow)]
    #[allow(unused)] // TODO: format this somehow
    target: Option<Cow<'a, str>>,
    #[serde(borrow)]
    filename: Option<Cow<'a, str>>,
    line_number: Option<u32>,
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
}

#[derive(serde::Serialize)]
struct LogsParams {
    num_lines: Option<u32>,
    follow: bool,
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let num_lines = args.get_one::<u32>("num_lines").copied();
    let database = args.get_one::<String>("database").unwrap();
    let follow = args.get_flag("follow");

    let cloned_config = config.clone();
    let identity = cloned_config.resolve_name_to_identity(args.get_one::<String>("identity").map(|x| x.as_str()));
    let auth_header = get_auth_header(&mut config, false, identity.as_deref())
        .await
        .map(|x| x.0);

    let address = database_address(&config, database).await?;

    // TODO: num_lines should default to like 10 if follow is specified?
    let query_parms = LogsParams { num_lines, follow };

    let builder = reqwest::Client::new().get(format!("{}/database/logs/{}", config.get_host_url(), address));
    let builder = add_auth_header_opt(builder, &auth_header);
    let res = builder.query(&query_parms).send().await?.error_for_status()?;

    let term_color = if std::io::stderr().is_terminal() {
        termcolor::ColorChoice::Auto
    } else {
        termcolor::ColorChoice::Never
    };
    let out = termcolor::StandardStream::stderr(term_color);
    let mut out = out.lock();

    let mut rdr = res
        .bytes_stream()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        .into_async_read();
    let mut line = String::new();
    while rdr.read_line(&mut line).await? != 0 {
        let record = serde_json::from_str::<Record<'_>>(&line)?;

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
        let dimmed = ColorSpec::new().set_dimmed(true).clone();
        if let Some(filename) = record.filename {
            out.set_color(&dimmed)?;
            write!(out, "{filename}")?;
            if let Some(line) = record.line_number {
                write!(out, ":{line}")?;
            }
            out.reset()?;
        }
        writeln!(out, ": {}", record.message)?;
        if let Some(trace) = &record.trace {
            for frame in trace {
                write!(out, "    in ")?;
                if let Some(module) = &frame.module_name {
                    out.set_color(&dimmed)?;
                    write!(out, "{module}")?;
                    out.reset()?;
                    write!(out, " :: ")?;
                }
                if let Some(function) = &frame.func_name {
                    out.set_color(&dimmed)?;
                    writeln!(out, "{function}")?;
                    out.reset()?;
                }
            }
        }

        line.clear();
    }

    Ok(())
}
