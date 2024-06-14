use std::collections::HashMap;
use std::iter;
use std::time::{Duration, Instant};

use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson};
use crate::format::{self, OutputFormat};
use clap::{value_parser, Arg, ArgAction, ArgGroup, ArgMatches};
use reqwest::RequestBuilder;
use serde_json::json;
use spacetimedb::json::client_api;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{satn, Typespace};
use tabled::settings::panel::Footer;
use tabled::settings::Style;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

use crate::config::Config;
use crate::errors::error_for_status;
use crate::util::{database_address, get_auth_header_only};

pub fn cli() -> clap::Command {
    clap::Command::new("sql")
        .about("Runs a SQL query on the database.")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database you would like to query"),
        )
        .arg(
            Arg::new("query")
                .action(ArgAction::Set)
                .required(true)
                .conflicts_with("interactive")
                .help("The SQL query to execute"),
        )
        .arg(Arg::new("interactive")
                 .long("interactive")
                 .action(ArgAction::SetTrue)
                 .conflicts_with("query")
                 .help("Runs an interactive command prompt for `SQL` expressions"),)
        .group(
            ArgGroup::new("mode")
                .args(["interactive","query"])
                .multiple(false)
                .required(true)
        )
        .arg(
            Arg::new("output-format")
                .long("output-format")
                .short('o')
                .conflicts_with("interactive")
                .value_parser(value_parser!(OutputFormat))
                .default_value("table")
                .help("How to render the output of queries. Not available in interactive mode.")
        )
        .arg(
            Arg::new("identity")
                .long("identity")
                .short('i')
                .conflicts_with("anon_identity")
                .help("The identity to use for querying the database")
                .long_help("The identity to use for querying the database. If no identity is provided, the default one will be used."),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("identity")
                .action(ArgAction::SetTrue)
                .help("If this flag is present, no identity will be provided when querying the database")
        )
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("The nickname, host name or URL of the server hosting the database"),
        )
}

pub(crate) async fn parse_req(mut config: Config, args: &ArgMatches) -> Result<Connection, anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let identity = args.get_one::<String>("identity");
    let anon_identity = args.get_flag("anon_identity");

    Ok(Connection {
        host: config.get_host_url(server)?,
        auth_header: get_auth_header_only(&mut config, anon_identity, identity, server).await?,
        address: database_address(&config, database, server).await?,
        database: database.to_string(),
    })
}

#[derive(Clone, Copy)]
pub struct RenderOpts {
    pub fmt: OutputFormat,
    pub with_timing: bool,
    pub with_row_count: bool,
}

struct Output<'a> {
    stmt_results: Vec<StmtResultJson<'a>>,
    elapsed: Duration,
}

impl Output<'_> {
    /// Render the [`Output`] to `out` according to [`RenderOpts`].
    async fn render(
        &self,
        out: impl AsyncWrite + Unpin,
        RenderOpts {
            fmt,
            with_timing,
            with_row_count,
        }: RenderOpts,
    ) -> anyhow::Result<()> {
        use OutputFormat::*;

        match fmt {
            Json => self.render_json(out).await,
            Table => self.render_tabled(out, with_timing, with_row_count).await,
            Csv => self.render_csv(out).await,
        }
    }

    /// Renders each query result as a JSON object, delimited as [`json-seq`].
    ///
    /// The objects have a single field `rows`, which is an array of the result
    /// values keyed by their column names.
    ///
    /// `json-seq` can be decoded by `jq --seq` in a streaming fashion.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message; SELECT * FROM Users;` against the
    /// `quickstart-chat` application may return the following:
    ///
    /// ```json
    /// {
    ///   "rows": [{
    ///     "text": "hello",
    ///     "sender": [
    ///       "aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"
    ///     ],
    ///     "sent": 1718611618273455
    ///   }]
    /// }
    /// {
    ///   "rows": [{
    ///     "identity": [
    ///       "aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"
    ///     ],
    ///     "name": {
    ///       "1": []
    ///     },
    ///     "online": true
    ///   }]
    /// }
    /// ```
    ///
    /// [json-seq]: https://datatracker.ietf.org/doc/html/rfc7464
    async fn render_json(&self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        for stmt_result in &self.stmt_results {
            let client_api::StmtResultJson { schema, rows } = stmt_result.try_into()?;

            let col_names = Vec::from(schema.elements)
                .into_iter()
                .enumerate()
                .map(|(pos, col)| col.name.unwrap_or(pos.to_string().into()))
                .collect::<Vec<_>>();
            let rows = rows
                .into_iter()
                .map(|row| col_names.iter().zip(Vec::from(row.elements)).collect::<HashMap<_, _>>())
                .collect::<Vec<_>>();

            format::write_json_seq(&mut out, &json!({"rows": rows})).await?;
        }
        out.flush().await?;

        Ok(())
    }

    /// Renders each query result as an ASCII table similar in style to `psql`.
    ///
    /// If `with_timing` is given, timing information about the whole request
    /// is printed after all tables have been rendered.
    ///
    /// If `with_row_count` is given, the number of rows is printed in the
    /// footer of each table.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message; SELECT * FROM Users;` against the
    /// `quickstart-chat` application may return the following:
    ///
    /// ```text
    ///  sender                                                             | sent             | text
    /// --------------------------------------------------------------------+------------------+----------------------
    ///  0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | 1718611618273455 | "hello"
    ///
    ///  identity                                                           | name        | online
    /// --------------------------------------------------------------------+-------------+--------
    ///  0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | (none = ()) | true
    /// ```
    async fn render_tabled(
        &self,
        mut out: impl AsyncWrite + Unpin,
        with_timing: bool,
        with_row_count: bool,
    ) -> anyhow::Result<()> {
        if self.stmt_results.is_empty() {
            if with_timing {
                out.write_all(self.fmt_timing().as_bytes()).await?;
            }
            out.write_all(b"OK").await?;
            return Ok(());
        }

        for (stmt_result, sep) in self.stmt_results.iter().zip(self.sep_by(b"\n\n")) {
            let mut table = stmt_result_to_table(stmt_result)?;
            if with_row_count {
                let rows = stmt_result.rows.len();
                table.with(Footer::new(format!(
                    "({} {})",
                    rows,
                    if rows == 1 { "row" } else { "rows" }
                )));
            }
            out.write_all(table.to_string().as_bytes()).await?;
            if let Some(sep) = sep {
                out.write_all(sep).await?;
            }
        }

        if with_timing {
            out.write_all(self.fmt_timing().as_bytes()).await?;
        }
        out.flush().await?;
        Ok(())
    }

    /// Renders each query result as a sequence of CSV-formatted lines.
    ///
    /// The column names are printed as a comment (a line starting with '#') at
    /// the beginning of each row.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message; SELECT * FROM Users;` against the
    /// `quickstart-chat` application may return the following:
    ///
    /// ```text
    /// # sender,sent,text
    /// 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,1718611618273455,hello
    /// # identity,name,online
    /// 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,(none = ()),true
    /// ```
    async fn render_csv(&self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        for stmt_result in &self.stmt_results {
            let mut csv = csv_async::AsyncWriter::from_writer(&mut out);
            let StmtResultJson { schema, rows } = stmt_result;
            let ts = Typespace::EMPTY.with_type(schema);

            // Render column names as a comment, e.g.
            // `# sender,sent,text`
            let mut cols = schema.elements.iter().enumerate();
            if let Some((i, col)) = cols.next() {
                match col.name() {
                    Some(name) => csv.write_field(format!("# {}", name)).await?,
                    None => csv.write_field(format!("# {}", i)).await?,
                }
            }
            for (i, col) in cols {
                match col.name() {
                    Some(name) => csv.write_field(name).await?,
                    None => csv.write_field(i.to_string()).await?,
                }
            }
            // Terminate record.
            csv.write_record(None::<&[u8]>).await?;

            for row in rows {
                let row = from_json_seed(row.get(), SeedWrapper(ts))?;
                for value in ts.with_values(&row) {
                    let fmt = satn::PsqlWrapper { ty: ts.ty(), value }.to_string();
                    // Remove quotes around string values to prevent quoting.
                    csv.write_field(fmt.trim_matches('"')).await?;
                }
                // Terminate record.
                csv.write_record(None::<&[u8]>).await?;
            }
            csv.flush().await?;
        }

        out.flush().await?;
        Ok(())
    }

    fn fmt_timing(&self) -> String {
        format!("Time: {:.2?}\n", self.elapsed)
    }

    fn sep_by<'a>(&self, s: &'a [u8]) -> impl Iterator<Item = Option<&'a [u8]>> {
        iter::repeat(Some(s))
            .take(self.stmt_results.len().saturating_sub(1))
            .chain([None])
    }
}

pub(crate) async fn run_sql(builder: RequestBuilder, sql: &str, opts: RenderOpts) -> Result<(), anyhow::Error> {
    let now = Instant::now();

    let json = error_for_status(builder.body(sql.to_owned()).send().await?)
        .await?
        .text()
        .await?;
    let stmt_results: Vec<StmtResultJson> = serde_json::from_str(&json)?;

    Output {
        stmt_results,
        elapsed: now.elapsed(),
    }
    .render(tokio::io::stdout(), opts)
    .await?;

    Ok(())
}

fn stmt_result_to_table(stmt_result: &StmtResultJson) -> anyhow::Result<tabled::Table> {
    let StmtResultJson { schema, rows } = stmt_result;

    let mut builder = tabled::builder::Builder::default();
    builder.push_record(
        schema
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| e.name.clone().unwrap_or_else(|| format!("column {i}").into())),
    );

    let ty = Typespace::EMPTY.with_type(schema);
    for row in rows {
        let row = from_json_seed(row.get(), SeedWrapper(ty))?;
        builder.push_record(
            ty.with_values(&row)
                .map(|value| satn::PsqlWrapper { ty: ty.ty(), value }.to_string()),
        );
    }

    let mut table = builder.build();
    table.with(Style::psql());

    Ok(table)
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let interactive = args.get_one::<bool>("interactive").unwrap_or(&false);
    if *interactive {
        let con = parse_req(config, args).await?;

        crate::repl::exec(con).await?;
    } else {
        let query = args.get_one::<String>("query").unwrap();
        let fmt = args
            .get_one::<OutputFormat>("output-format")
            .copied()
            .unwrap_or(OutputFormat::Table);

        let con = parse_req(config, args).await?;
        let api = ClientApi::new(con);

        let render_opts = RenderOpts {
            fmt,
            with_timing: false,
            with_row_count: false,
        };
        run_sql(api.sql(), query, render_opts).await?;
    }
    Ok(())
}
