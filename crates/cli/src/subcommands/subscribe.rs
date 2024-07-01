use std::iter;
use std::time::Duration;

use anyhow::Context;
use clap::{value_parser, Arg, ArgAction, ArgMatches};
use futures::{Sink, SinkExt, TryStream, TryStreamExt};
use http::header;
use http::uri::Scheme;
use serde::de::DeserializeSeed;
use serde_json::value::RawValue;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::de::ProductVisitor;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::{sats::satn, ModuleDef};
use spacetimedb_standalone::TEXT_PROTOCOL;
use tabled::settings::panel::Footer;
use tabled::settings::Style;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::api::ClientApi;
use crate::format::{self, OutputFormat};
use crate::sql::parse_req;
use crate::Config;

pub fn cli() -> clap::Command {
    clap::Command::new("subscribe")
        .about("Subscribe to SQL queries on the database.")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database you would like to query"),
        )
        .arg(
            Arg::new("query")
                .required(true)
                .num_args(1..)
                .help("The SQL query to execute"),
        )
        .arg(
            Arg::new("num-updates")
                .required(false)
                .short('n')
                .action(ArgAction::Set)
                .value_parser(value_parser!(u32))
                .help("The number of subscription updates to receive before exiting"),
        )
        .arg(
            Arg::new("timeout")
                .required(false)
                .short('t')
                .long("timeout")
                .action(ArgAction::Set)
                .value_parser(value_parser!(u32))
                .help(
                    "The timeout, in seconds, after which to disconnect and stop receiving \
                     subscription messages. If `-n` is specified, it will stop after whichever
                     one comes first.",
                ),
        )
        .arg(
            Arg::new("print_initial_update")
                .required(false)
                .long("print-initial-update")
                .action(ArgAction::SetTrue)
                .help("Print the initial update for the queries."),
        )
        .arg(
            Arg::new("output-format")
                .long("output-format")
                .short('o')
                .value_parser(value_parser!(OutputFormat))
                .default_value("json")
                .help("How to format the subscription updates."),
        )
        .arg(
            Arg::new("identity")
                .long("identity")
                .short('i')
                .conflicts_with("anon_identity")
                .help("The identity to use for querying the database")
                .long_help(
                    "The identity to use for querying the database. \
                     If no identity is provided, the default one will be used.",
                ),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("identity")
                .action(ArgAction::SetTrue)
                .help("If this flag is present, no identity will be provided when querying the database"),
        )
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("The nickname, host name or URL of the server hosting the database"),
        )
}

#[derive(serde::Serialize)]
enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe { query_strings: Vec<String> },
}

fn parse_msg_json(msg: &WsMessage) -> Option<ServerMessage> {
    let WsMessage::Text(msg) = msg else { return None };
    serde_json::from_str(msg)
        .inspect_err(|e| eprintln!("couldn't parse message from server: {e}"))
        .ok()
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TableUpdateJson<'a> {
    table_name: Box<str>,
    #[serde(borrow)]
    table_row_operations: Vec<TableRowOperationJson<'a>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TableRowOperationJson<'a> {
    op: Op,
    #[serde(borrow)]
    row: &'a RawValue, // ProductValue
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Op {
    Delete,
    Insert,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SubscriptionUpdateJson<'a> {
    #[serde(borrow)]
    table_updates: Vec<TableUpdateJson<'a>>,
}

impl SubscriptionUpdateJson<'_> {
    /// Render the update to `out`, resolving row types using `schema`, and
    /// formatting the output according to `fmt`.
    async fn render(self, out: impl AsyncWrite + Unpin, schema: &ModuleDef, fmt: OutputFormat) -> anyhow::Result<()> {
        use OutputFormat::*;

        match fmt {
            Json => self.render_json(out, schema).await,
            Table => self.render_tabled(out, schema).await,
            Csv => self.render_csv(out, schema).await,
        }
    }

    /// Renders the update as a JSON object, delimited as [`json-seq`].
    ///
    /// `json-seq` can be decoded by `jq --seq` in a streaming fashion.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message` against the `quickstart-chat` application
    /// may yield the following updates (without `--print-initial-update`):
    ///
    /// ```json
    /// {"Message":{"deletes":[],"inserts":[{"sender":{"__identity_bytes":"aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"},"sent":1718614904711241,"text":"hallo"}]}}
    /// {"Message":{"deletes":[],"inserts":[{"sender":{"__identity_bytes":"aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"},"sent":1718614913023180,"text":"wie geht's"}]}}
    /// {"Message":{"deletes":[],"inserts":[{"sender":{"__identity_bytes":"aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"},"sent":1718614919407019,"text":":ghost:"}]}}/
    /// ```
    ///
    /// [json-seq]: https://datatracker.ietf.org/doc/html/rfc7464
    async fn render_json(self, mut out: impl AsyncWrite + Unpin, schema: &ModuleDef) -> anyhow::Result<()> {
        let tables: HashMap<Box<str>, SubscriptionTable> = self
            .table_updates
            .into_iter()
            .map(|upd| {
                let table_schema = schema
                    .tables
                    .iter()
                    .find(|tbl| tbl.schema.table_name == upd.table_name)
                    .with_context(|| format!("table `{}` not found in schema", upd.table_name))?;
                let mut deletes = Vec::new();
                let mut inserts = Vec::new();
                let table_ty = schema.typespace.resolve(table_schema.data);
                for op in upd.table_row_operations {
                    let row = DeserializeSeed::deserialize(SeedWrapper(table_ty), op.row)?;
                    let row = table_ty.with_value(&row);
                    let row = serde_json::to_value(SerializeWrapper::from_ref(&row))?;
                    match op.op {
                        Op::Delete => deletes.push(row),
                        Op::Insert => inserts.push(row),
                    }
                }
                Ok((upd.table_name.clone(), SubscriptionTable { deletes, inserts }))
            })
            .collect::<anyhow::Result<_>>()?;

        format::write_json_seq(&mut out, &tables).await?;
        out.flush().await?;

        Ok(())
    }

    /// Renders the update as ASCII tables similar in style to `psql`.
    ///
    /// For each database table in the update, a separate ASCII table is drawn.
    ///
    /// The first column of each table indicates the operation which was
    /// performed on the row: 'I' for insert, or 'D' for delete.
    /// The table footer indicates the table name and how many deletes and
    /// inserts where in the update, respectively.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM *` agains the `quickstart-chat` application may
    /// yield the following updates (without `--print-initial-update`):
    ///
    /// ```text
    ///    | sender                                                             | sent             | text
    /// ---+--------------------------------------------------------------------+------------------+---------
    ///  I | 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | 1718615094012264 | "hallo"
    ///  Table: Message, Deletes: 0, Inserts: 1
    ///
    ///    | identity                                                           | name        | online
    /// ---+--------------------------------------------------------------------+-------------+--------
    ///  D | 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | (none = ()) | true
    ///  I | 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | (none = ()) | false
    ///  Table: User, Deletes: 1, Inserts: 1
    ///  ```
    async fn render_tabled(self, mut out: impl AsyncWrite + Unpin, schema: &ModuleDef) -> anyhow::Result<()> {
        for TableUpdateJson {
            table_name,
            table_row_operations,
        } in self.table_updates
        {
            let table_schema = schema
                .tables
                .iter()
                .find(|tbl| tbl.schema.table_name == table_name)
                .with_context(|| format!("table `{table_name}` not found in schema"))?;
            let table_ty = schema
                .typespace
                .resolve(table_schema.data)
                .map(|t| t.as_product().unwrap());

            let mut builder = {
                let rows = table_row_operations.len();
                // We need to make space for the `I / D` column.
                let cols = table_ty.product_len() + 1;
                tabled::builder::Builder::with_capacity(rows, cols)
            };
            builder.push_record(
                iter::once("").chain(
                    table_schema
                        .schema
                        .columns
                        .iter()
                        .map(|col_def| col_def.col_name.as_ref()),
                ),
            );

            let mut deletes = 0;
            let mut inserts = 0;
            for TableRowOperationJson { op, row } in table_row_operations {
                let op = match op {
                    Op::Delete => {
                        deletes += 1;
                        "D"
                    }
                    Op::Insert => {
                        inserts += 1;
                        "I"
                    }
                };
                let row = DeserializeSeed::deserialize(SeedWrapper(table_ty), row)?;
                let record = iter::once(op.into()).chain(table_ty.with_values(&row).map(|value| {
                    satn::PsqlWrapper {
                        ty: table_ty.ty(),
                        value,
                    }
                    .to_string()
                }));
                builder.push_record(record);
            }
            let mut rendered_table = builder
                .build()
                .with(Style::psql())
                .with(Footer::new(format!(
                    "Table: {}, Deletes: {}, Inserts: {}",
                    table_name, deletes, inserts,
                )))
                .to_string();
            rendered_table.push_str("\n\n");
            out.write_all(rendered_table.as_bytes()).await?;
        }
        out.flush().await?;

        Ok(())
    }

    /// Renders the update in CSV format.
    ///
    /// The first column on each line is the table name, followed by the
    /// operation ('I' for insert, or 'D' for delete), followed by the row as
    /// returned by the query.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM *` agains the `quickstart-chat` application may
    /// yield the following updates (without `--print-initial-update`):
    ///
    /// ```text
    /// Message,I,0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,1718615221730361,hallo
    /// User,D,0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,(none = ()),true
    /// User,I,0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,(none = ()),false
    /// ```
    async fn render_csv(self, mut out: impl AsyncWrite + Unpin, schema: &ModuleDef) -> anyhow::Result<()> {
        for TableUpdateJson {
            table_name,
            table_row_operations,
        } in self.table_updates
        {
            let mut csv = csv_async::AsyncWriter::from_writer(&mut out);
            let table_schema = schema
                .tables
                .iter()
                .find(|tbl| tbl.schema.table_name == table_name)
                .with_context(|| format!("table `{table_name}` not found in schema"))?;
            let table_ty = schema
                .typespace
                .resolve(table_schema.data)
                .map(|t| t.as_product().unwrap());

            for TableRowOperationJson { op, row } in table_row_operations {
                let op = match op {
                    Op::Delete => "D",
                    Op::Insert => "I",
                };
                let row = DeserializeSeed::deserialize(SeedWrapper(table_ty), row)?;

                csv.write_field(table_name.as_ref()).await?;
                csv.write_field(op).await?;
                for value in table_ty.with_values(&row) {
                    let fmt = satn::PsqlWrapper {
                        ty: table_ty.ty(),
                        value,
                    }
                    .to_string();
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
}

#[derive(serde::Serialize)]
struct SubscriptionTable {
    deletes: Vec<serde_json::Value>,
    inserts: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct Event {
    message: String,
}

#[derive(serde::Deserialize)]
enum ServerMessage<'a> {
    SubscriptionUpdate(#[serde(borrow)] SubscriptionUpdateJson<'a>),
    TransactionUpdate {
        subscription_update: SubscriptionUpdateJson<'a>,
        event: Event,
    },
    #[serde(other, deserialize_with = "serde_with::rust::deserialize_ignore_any")]
    Unknown,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let queries = args.get_many::<String>("query").unwrap();
    let num = args.get_one::<u32>("num-updates").copied();
    let timeout = args.get_one::<u32>("timeout").copied();
    let print_initial_update = args.get_flag("print_initial_update");
    let fmt = args
        .get_one::<OutputFormat>("output-format")
        .copied()
        .unwrap_or(OutputFormat::Json);

    let conn = parse_req(config, args).await?;
    let api = ClientApi::new(conn);
    let module_def = api.module_def().await?;

    // Change the URI scheme from `http(s)` to `ws(s)`.
    let mut uri = http::Uri::try_from(api.con.db_uri("subscribe")).unwrap().into_parts();
    uri.scheme = uri.scheme.map(|s| {
        if s == Scheme::HTTP {
            "ws".parse().unwrap()
        } else if s == Scheme::HTTPS {
            "wss".parse().unwrap()
        } else {
            s
        }
    });

    // Create the websocket request.
    let mut req = http::Uri::from_parts(uri).unwrap().into_client_request()?;
    req.headers_mut().insert(header::SEC_WEBSOCKET_PROTOCOL, TEXT_PROTOCOL);
    //  Add the authorization header, if any.
    if let Some(auth_header) = &api.con.auth_header {
        req.headers_mut()
            .insert(header::AUTHORIZATION, auth_header.try_into().unwrap());
    }
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await?;

    let task = async {
        subscribe(&mut ws, queries.cloned().collect()).await?;
        await_initial_update(&mut ws, print_initial_update.then_some((&module_def, fmt))).await?;
        consume_transaction_updates(&mut ws, num, &module_def, fmt).await
    };

    let needs_shutdown = if let Some(timeout) = timeout {
        let timeout = Duration::from_secs(timeout.into());
        match tokio::time::timeout(timeout, task).await {
            Ok(res) => res?,
            Err(_elapsed) => true,
        }
    } else {
        task.await?
    };

    if needs_shutdown {
        ws.close(None).await?;
    }

    Ok(())
}

/// Send the subscribe message.
async fn subscribe<S>(ws: &mut S, query_strings: Vec<String>) -> Result<(), S::Error>
where
    S: Sink<WsMessage> + Unpin,
{
    let msg = serde_json::to_string(&ClientMessage::Subscribe { query_strings }).unwrap();
    ws.send(msg.into()).await
}

/// Await the initial [`ServerMessage::SubscriptionUpdate`].
/// If `print` is `Some`, the update is printed to stdout according to the
/// contained schema and output format.
async fn await_initial_update<S>(ws: &mut S, print: Option<(&ModuleDef, OutputFormat)>) -> anyhow::Result<()>
where
    S: TryStream<Ok = WsMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    while let Some(msg) = ws.try_next().await? {
        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ServerMessage::SubscriptionUpdate(sub) => {
                if let Some((schema, fmt)) = print {
                    sub.render(tokio::io::stdout(), schema, fmt).await?;
                }
                break;
            }
            ServerMessage::TransactionUpdate { event, .. } => {
                let mut message = event.message;
                if message.is_empty() {
                    message.push_str("protocol error: received transaction update before initial subscription update");
                }
                anyhow::bail!(message)
            }
            ServerMessage::Unknown => continue,
        }
    }

    Ok(())
}

/// Print `num` [`ServerMessage::TransactionUpdate`] messages as JSON.
/// If `num` is `None`, keep going indefinitely.
/// Received updates are printed to stdout according to `schema` and `fmt`.
async fn consume_transaction_updates<S>(
    ws: &mut S,
    num: Option<u32>,
    schema: &ModuleDef,
    fmt: OutputFormat,
) -> anyhow::Result<bool>
where
    S: TryStream<Ok = WsMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let mut num_received = 0;
    loop {
        if num.is_some_and(|n| num_received >= n) {
            break Ok(true);
        }
        let Some(msg) = ws.try_next().await? else {
            eprintln!("disconnected by server");
            break Ok(false);
        };

        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ServerMessage::SubscriptionUpdate(_) => {
                anyhow::bail!("protocol error: received a second initial subscription update")
            }
            ServerMessage::TransactionUpdate {
                subscription_update, ..
            } => {
                subscription_update.render(tokio::io::stdout(), schema, fmt).await?;
                num_received += 1;
            }
            ServerMessage::Unknown => continue,
        }
    }
}
