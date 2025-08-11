use anyhow::Context;
use clap::{value_parser, Arg, ArgAction, ArgMatches};
use futures::{Sink, SinkExt, TryStream, TryStreamExt};
use http::header;
use http::uri::Scheme;
use serde_json::Value;
use spacetimedb_client_api_messages::websocket::{self as ws, JsonFormat};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::de::serde::{DeserializeWrapper, SeedWrapper};
use spacetimedb_lib::ser::serde::SerializeWrapper;
use std::io;
use std::time::Duration;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};

use crate::api::ClientApi;
use crate::common_args;
use crate::sql::parse_req;
use crate::util::UNSTABLE_WARNING;
use crate::Config;

pub fn cli() -> clap::Command {
    clap::Command::new("subscribe")
        .about(format!("Subscribe to SQL queries on the database. {UNSTABLE_WARNING}"))
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database you would like to query"),
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
                .long("num-updates")
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
        .arg(common_args::anonymous())
        .arg(common_args::yes())
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
}

fn parse_msg_json(msg: &WsMessage) -> Option<ws::ServerMessage<JsonFormat>> {
    let WsMessage::Text(msg) = msg else { return None };
    serde_json::from_str::<DeserializeWrapper<ws::ServerMessage<JsonFormat>>>(msg)
        .inspect_err(|e| eprintln!("couldn't parse message from server: {e}"))
        .map(|wrapper| wrapper.0)
        .ok()
}

fn reformat_update<'a>(
    msg: &'a ws::DatabaseUpdate<JsonFormat>,
    schema: &RawModuleDefV9,
) -> anyhow::Result<HashMap<&'a str, SubscriptionTable>> {
    msg.tables
        .iter()
        .map(|upd| {
            let table_schema = schema
                .tables
                .iter()
                .find(|tbl| tbl.name == upd.table_name)
                .context("table not found in schema")?;
            let table_ty = schema.typespace.resolve(table_schema.product_type_ref);

            let reformat_row = |row: &str| -> anyhow::Result<Value> {
                // TODO: can the following two calls be merged into a single call to reduce allocations?
                let row = serde_json::from_str::<Value>(row)?;
                let row = serde::de::DeserializeSeed::deserialize(SeedWrapper(table_ty), row)?;
                let row = table_ty.with_value(&row);
                let row = serde_json::to_value(SerializeWrapper::from_ref(&row))?;
                Ok(row)
            };

            let mut deletes = Vec::new();
            let mut inserts = Vec::new();
            for upd in &upd.updates {
                for s in &upd.deletes {
                    deletes.push(reformat_row(s)?);
                }
                for s in &upd.inserts {
                    inserts.push(reformat_row(s)?);
                }
            }

            Ok((&*upd.table_name, SubscriptionTable { deletes, inserts }))
        })
        .collect()
}

#[derive(serde::Serialize, Debug)]
struct SubscriptionTable {
    deletes: Vec<serde_json::Value>,
    inserts: Vec<serde_json::Value>,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let queries = args.get_many::<String>("query").unwrap();
    let num = args.get_one::<u32>("num-updates").copied();
    let timeout = args.get_one::<u32>("timeout").copied();
    let print_initial_update = args.get_flag("print_initial_update");

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
    req.headers_mut().insert(
        header::SEC_WEBSOCKET_PROTOCOL,
        http::HeaderValue::from_static(ws::TEXT_PROTOCOL),
    );
    //  Add the authorization header, if any.
    if let Some(auth_header) = api.con.auth_header.to_header() {
        req.headers_mut().insert(header::AUTHORIZATION, auth_header);
    }
    let mut ws = tokio_tungstenite::connect_async(req).await.map(|(ws, _)| ws)?;

    let task = async {
        subscribe(&mut ws, queries.cloned().map(Into::into).collect()).await?;
        await_initial_update(&mut ws, print_initial_update.then_some(&module_def)).await?;
        consume_transaction_updates(&mut ws, num, &module_def).await
    };

    let res = if let Some(timeout) = timeout {
        let timeout = Duration::from_secs(timeout.into());
        match tokio::time::timeout(timeout, task).await {
            Ok(res) => res,
            Err(_elapsed) => {
                eprintln!("timed out after {}s", timeout.as_secs());
                Ok(())
            }
        }
    } else {
        task.await
    };

    // Close the connection gracefully.
    // This will return an error if the server already closed,
    // or the connection is in a bad state.
    // The error (if any) relevant to the user is already stored in `res`,
    // so we can ignore errors here -- graceful close is basically a
    // courtesy to the server.
    let _ = ws.close(None).await;
    // The server closing the connection is not considered an error,
    // but any other error is.
    res.or_else(|e| {
        if e.is_server_closed_connection() {
            Ok(())
        } else {
            Err(e)
        }
    })
    .map_err(anyhow::Error::from)
}

#[derive(Debug, Error)]
enum Error {
    #[error("error sending subscription queries")]
    Subscribe {
        #[source]
        source: WsError,
    },
    #[error("protocol error: {details}")]
    Protocol { details: &'static str },
    #[error("websocket error: {source}")]
    Websocket {
        #[source]
        source: WsError,
    },
    #[error("encountered failed transaction: {reason}")]
    TransactionFailure { reason: Box<str> },
    #[error("error formatting response: {source:#}")]
    Reformat {
        #[source]
        source: anyhow::Error,
    },
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl Error {
    fn is_server_closed_connection(&self) -> bool {
        matches!(
            self,
            Self::Websocket {
                source: WsError::ConnectionClosed
            }
        )
    }
}

/// Send the subscribe message.
async fn subscribe<S>(ws: &mut S, query_strings: Box<[Box<str>]>) -> Result<(), Error>
where
    S: Sink<WsMessage, Error = WsError> + Unpin,
{
    let msg = serde_json::to_string(&SerializeWrapper::new(ws::ClientMessage::<()>::Subscribe(
        ws::Subscribe {
            query_strings,
            request_id: 0,
        },
    )))
    .unwrap();
    ws.send(msg.into()).await.map_err(|source| Error::Subscribe { source })
}

/// Await the initial [`ServerMessage::SubscriptionUpdate`].
/// If `module_def` is `Some`, print a JSON representation to stdout.
async fn await_initial_update<S>(ws: &mut S, module_def: Option<&RawModuleDefV9>) -> Result<(), Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    const RECV_TX_UPDATE: &str = "protocol error: received transaction update before initial subscription update";

    while let Some(msg) = ws.try_next().await.map_err(|source| Error::Websocket { source })? {
        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws::ServerMessage::InitialSubscription(sub) => {
                if let Some(module_def) = module_def {
                    let output = format_output_json(&sub.database_update, module_def)?;
                    tokio::io::stdout().write_all(output.as_bytes()).await?
                }
                break;
            }
            ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate { status, .. }) => {
                return Err(match status {
                    ws::UpdateStatus::Failed(msg) => Error::TransactionFailure { reason: msg },
                    _ => Error::Protocol {
                        details: RECV_TX_UPDATE,
                    },
                })
            }
            ws::ServerMessage::TransactionUpdateLight(ws::TransactionUpdateLight { .. }) => {
                return Err(Error::Protocol {
                    details: RECV_TX_UPDATE,
                })
            }
            _ => continue,
        }
    }

    Ok(())
}

/// Print `num` [`ServerMessage::TransactionUpdate`] messages as JSON.
/// If `num` is `None`, keep going indefinitely.
async fn consume_transaction_updates<S>(ws: &mut S, num: Option<u32>, module_def: &RawModuleDefV9) -> Result<(), Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    let mut stdout = tokio::io::stdout();
    let mut num_received = 0;
    loop {
        if num.is_some_and(|n| num_received >= n) {
            return Ok(());
        }
        let Some(msg) = ws.try_next().await.map_err(|source| Error::Websocket { source })? else {
            eprintln!("disconnected by server");
            return Err(Error::Websocket {
                source: WsError::ConnectionClosed,
            });
        };

        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws::ServerMessage::InitialSubscription(_) => {
                return Err(Error::Protocol {
                    details: "received a second initial subscription update",
                })
            }
            ws::ServerMessage::TransactionUpdateLight(ws::TransactionUpdateLight { update, .. })
            | ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate {
                status: ws::UpdateStatus::Committed(update),
                ..
            }) => {
                let output = format_output_json(&update, module_def)?;
                stdout.write_all(output.as_bytes()).await?;
                num_received += 1;
            }
            _ => continue,
        }
    }
}

fn format_output_json(msg: &ws::DatabaseUpdate<JsonFormat>, schema: &RawModuleDefV9) -> Result<String, Error> {
    let formatted = reformat_update(msg, schema).map_err(|source| Error::Reformat { source })?;
    let output = serde_json::to_string(&formatted)? + "\n";

    Ok(output)
}
