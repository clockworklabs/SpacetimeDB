use anyhow::{bail, Context};
use clap::{value_parser, Arg, ArgAction, ArgMatches};
use futures::{Sink, SinkExt, TryStream, TryStreamExt};
use http::header;
use http::uri::Scheme;
use spacetimedb_client_api_messages::websocket::{self as ws, EncodedValue};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::de::serde::{DeserializeWrapper, SeedWrapper};
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::{ModuleDef, TableDesc};
use spacetimedb_standalone::TEXT_PROTOCOL;
use std::iter;
use std::time::Duration;
use tabled::settings::panel::Footer;
use tabled::settings::Style;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::api::{from_json_seed, ClientApi};
use crate::format::{self, arg_output_format, fmt_row_psql, get_arg_output_format, OutputFormat, Render};
use crate::sql::parse_req;
use crate::{common_args, Config};

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
        .arg(arg_output_format("json").long_help("How to render the subscription updates."))
        .arg(
            common_args::identity()
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
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
}

fn parse_msg_json(msg: &WsMessage) -> Option<ws::ServerMessage> {
    let WsMessage::Text(msg) = msg else { return None };
    serde_json::from_str::<DeserializeWrapper<ws::ServerMessage>>(msg)
        .inspect_err(|e| eprintln!("couldn't parse message from server: {e}"))
        .map(|wrapper| wrapper.0)
        .ok()
}

#[derive(serde::Serialize, Debug)]
struct SubscriptionTable {
    deletes: Vec<serde_json::Value>,
    inserts: Vec<serde_json::Value>,
}

struct Output<'a> {
    schema: &'a ModuleDef,
    table_updates: Vec<ws::TableUpdate>,
    /// If true, omit the I / D column in tabled output.
    is_initial_update: bool,
}

impl Output<'_> {
    pub async fn render(self, fmt: OutputFormat, out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        format::render(self, fmt, out).await
    }
}

fn find_table_schema<'a>(schema: &'a ModuleDef, table_name: &str) -> anyhow::Result<&'a TableDesc> {
    schema
        .tables
        .iter()
        .find(|tbl| &*tbl.schema.table_name == table_name)
        .with_context(|| format!("table `{}` not found in schema", table_name))
}

fn decode_row<'de, T: DeserializeSeed<'de>>(row: &'de EncodedValue, seed: T) -> anyhow::Result<T::Output> {
    let EncodedValue::Text(row) = row else {
        bail!("Expected row in text format but found {row:?}");
    };
    Ok(from_json_seed(row, SeedWrapper(seed))?)
}

impl Render for Output<'_> {
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
    async fn render_json(self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        let tables: HashMap<String, SubscriptionTable> = self
            .table_updates
            .into_iter()
            .map(|upd| {
                let table_schema = find_table_schema(self.schema, &upd.table_name)?;
                let table_ty = self.schema.typespace.resolve(table_schema.data);

                let reformat_row = |row: EncodedValue| {
                    let row = decode_row(&row, table_ty)?;
                    let row = table_ty.with_value(&row);
                    let row = serde_json::to_value(SerializeWrapper::from_ref(&row))?;
                    Ok(row)
                };

                let deletes = upd
                    .deletes
                    .into_iter()
                    .map(reformat_row)
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let inserts = upd
                    .inserts
                    .into_iter()
                    .map(reformat_row)
                    .collect::<anyhow::Result<Vec<_>>>()?;

                Ok((upd.table_name, SubscriptionTable { deletes, inserts }))
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
    ///  Table: Message, (D)eletes: 0, (I)nserts: 1
    ///
    ///    | identity                                                           | name        | online
    /// ---+--------------------------------------------------------------------+-------------+--------
    ///  D | 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | (none = ()) | true
    ///  I | 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | (none = ()) | false
    ///  Table: User, (D)eletes: 1, (I)nserts: 1
    ///  ```
    async fn render_tabled(self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        for ws::TableUpdate {
            table_name,
            deletes,
            inserts,
            ..
        } in self.table_updates
        {
            let table_schema = find_table_schema(self.schema, &table_name)?;
            let table_ty = self
                .schema
                .typespace
                .resolve(table_schema.data)
                .map(|t| t.as_product().unwrap());

            let mut builder = {
                let rows = deletes.len() + inserts.len();
                let mut cols = table_schema.schema.columns.len();
                if !self.is_initial_update {
                    // We need to make space for the `I / D` column.
                    cols += 1;
                }
                tabled::builder::Builder::with_capacity(rows, cols)
            };
            let column_names = table_schema
                .schema
                .columns
                .iter()
                .map(|col_def| col_def.col_name.as_ref());
            if self.is_initial_update {
                builder.push_record(column_names)
            } else {
                // Empty header for `I / D` column.
                builder.push_record(iter::once("").chain(column_names))
            };

            let n_deletes = deletes.len();
            let n_inserts = inserts.len();

            for (row, op) in deletes
                .into_iter()
                .zip(iter::repeat("D"))
                .chain(inserts.into_iter().zip(iter::repeat("I")))
            {
                let row = decode_row(&row, table_ty)?;
                builder.push_record(iter::once(op.into()).chain(fmt_row_psql(&row, table_ty)));
            }

            let mut table = builder.build();
            table.with(Style::psql());
            if self.is_initial_update && n_deletes == 0 {
                table.with(Footer::new(format!("Table: {}, Rows: {}", table_name, n_inserts)));
            } else {
                table.with(Footer::new(format!(
                    "Table: {}, (D)eletes: {}, (I)nserts: {}",
                    table_name, n_deletes, n_inserts,
                )));
            };
            out.write_all(table.to_string().as_bytes()).await?;
            out.write_all(b"\n\n").await?;
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
    async fn render_csv(self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        for ws::TableUpdate {
            table_name,
            deletes,
            inserts,
            ..
        } in self.table_updates
        {
            let table_schema = find_table_schema(self.schema, &table_name)?;
            let table_ty = self
                .schema
                .typespace
                .resolve(table_schema.data)
                .map(|t| t.as_product().unwrap());

            let mut csv = csv_async::AsyncWriter::from_writer(&mut out);

            for (row, op) in deletes
                .into_iter()
                .zip(iter::repeat("D"))
                .chain(inserts.into_iter().zip(iter::repeat("I")))
            {
                let row = decode_row(&row, table_ty)?;
                csv.write_field(&table_name).await?;
                csv.write_field(op).await?;
                for field in fmt_row_psql(&row, table_ty) {
                    // Remove quotes around string values to prevent `csv` from
                    // quoting already quoted strings.
                    csv.write_field(field.trim_matches('"')).await?;
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

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let queries = args.get_many::<String>("query").unwrap();
    let num = args.get_one::<u32>("num-updates").copied();
    let timeout = args.get_one::<u32>("timeout").copied();
    let print_initial_update = args.get_flag("print_initial_update");
    let fmt = get_arg_output_format(args);

    let conn = parse_req(config, args).await?;
    let api = ClientApi::new(conn);
    let module_def = api.module_def().await?;

    // Change the URI scheme from `http(s)` to `ws(s)`.
    let mut uri = http::Uri::try_from(api.con.db_uri("subscribe"))?.into_parts();
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
    let mut req = http::Uri::from_parts(uri)?.into_client_request()?;
    req.headers_mut().insert(header::SEC_WEBSOCKET_PROTOCOL, TEXT_PROTOCOL);
    //  Add the authorization header, if any.
    if let Some(auth_header) = &api.con.auth_header {
        req.headers_mut().insert(header::AUTHORIZATION, auth_header.try_into()?);
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
    let msg = serde_json::to_string(&SerializeWrapper::new(ws::ClientMessage::<()>::Subscribe(
        ws::Subscribe {
            query_strings,
            request_id: 0,
        },
    )))
    .unwrap();
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
            ws::ServerMessage::InitialSubscription(sub) => {
                if let Some((schema, fmt)) = print {
                    Output {
                        schema,
                        table_updates: sub.database_update.tables,
                        is_initial_update: true,
                    }
                    .render(fmt, tokio::io::stdout())
                    .await?;
                }
                break;
            }
            ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate { status, .. }) => {
                let message = match status {
                    ws::UpdateStatus::Failed(msg) => msg,
                    _ => "protocol error: received transaction update before initial subscription update".to_string(),
                };
                anyhow::bail!(message)
            }
            _ => continue,
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
            ws::ServerMessage::InitialSubscription(_) => {
                anyhow::bail!("protocol error: received a second initial subscription update")
            }
            ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate {
                status: ws::UpdateStatus::Committed(update),
                ..
            }) => {
                Output {
                    schema,
                    table_updates: update.tables,
                    is_initial_update: false,
                }
                .render(fmt, tokio::io::stdout())
                .await?;
                num_received += 1;
            }
            _ => continue,
        }
    }
}
