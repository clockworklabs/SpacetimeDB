use std::time::Duration;
use anyhow::Context;
use bytes::Bytes;
use clap::{value_parser, Arg, ArgAction, ArgMatches};
use futures::{Sink, SinkExt, TryStream, TryStreamExt};
use http::header;
use http::uri::Scheme;
use serde_json::value::RawValue;
use serde_json::Value;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::de::serde::{DeserializeWrapper, SeedWrapper};
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::ModuleDef;
use spacetimedb_standalone::TEXT_PROTOCOL;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use spacetimedb_client_api_messages::websocket as ws;

use crate::api::ClientApi;
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

fn parse_msg_json(msg: &WsMessage) -> Option<ws::ServerMessage> {
    let WsMessage::Text(msg) = msg else { return None };
    serde_json::from_str::<DeserializeWrapper<ws::ServerMessage>>(msg)
        .inspect_err(|e| eprintln!("couldn't parse message from server: {e}"))
        .map(|wrapper| wrapper.0)
        .ok()
}

fn reformat_update(msg: ws::DatabaseUpdate, schema: &ModuleDef) -> anyhow::Result<HashMap<String, SubscriptionTable>> {
    msg.tables.into_iter().map(|upd| {
        let table_schema = schema
            .tables
            .iter()
            .find(|tbl| tbl.schema.table_name.as_ref() == &upd.table_name)
            .context("table not found in schema")?;
        let table_ty = schema.typespace.resolve(table_schema.data);

        let reformat_row = |row: Bytes| {
            let row = serde_json::from_slice::<Value>(&row)?;
            let row = serde::de::DeserializeSeed::deserialize(SeedWrapper(table_ty), row)?;
            let row = table_ty.with_value(&row);
            let row = serde_json::to_value(SerializeWrapper::from_ref(&row))?;
            Ok(row)   
        };
        
        let deletes = upd.deletes.into_iter().map(reformat_row).collect::<anyhow::Result<Vec<_>>>().unwrap();
        let inserts = upd.inserts.into_iter().map(reformat_row).collect::<anyhow::Result<Vec<_>>>().unwrap();
        
        Ok((upd.table_name.into(), SubscriptionTable { deletes, inserts }))
    }).collect()
}

#[derive(serde::Serialize, Debug)]
struct SubscriptionTable {
    deletes: Vec<serde_json::Value>,
    inserts: Vec<serde_json::Value>,
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let queries = args.get_many::<String>("query").unwrap();
    let num = args.get_one::<u32>("num-updates").copied();
    let timeout = args.get_one::<u32>("timeout").copied();
    let print_initial_update = args.get_flag("print_initial_update");

    let conn = parse_req(config, args).await.unwrap();
    let api = ClientApi::new(conn);
    let module_def = api.module_def().await.unwrap();

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
    let mut req = http::Uri::from_parts(uri).unwrap().into_client_request().unwrap();
    req.headers_mut().insert(header::SEC_WEBSOCKET_PROTOCOL, TEXT_PROTOCOL);
    //  Add the authorization header, if any.
    if let Some(auth_header) = &api.con.auth_header {
        req.headers_mut()
            .insert(header::AUTHORIZATION, auth_header.try_into().unwrap());
    }
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();

    let task = async {
        subscribe(&mut ws, queries.cloned().collect()).await.unwrap();
        await_initial_update(&mut ws, print_initial_update.then_some(&module_def)).await.unwrap();
        consume_transaction_updates(&mut ws, num, &module_def).await
    };

    let needs_shutdown = if let Some(timeout) = timeout {
        let timeout = Duration::from_secs(timeout.into());
        match tokio::time::timeout(timeout, task).await {
            Ok(res) => res.unwrap(),
            Err(_elapsed) => true,
        }
    } else {
        task.await.unwrap()
    };

    if needs_shutdown {
        ws.close(None).await.unwrap();
    }

    Ok(())
}

/// Send the subscribe message.
async fn subscribe<S>(ws: &mut S, query_strings: Vec<String>) -> Result<(), S::Error>
where
    S: Sink<WsMessage> + Unpin,
{
    let msg = serde_json::to_string(&SerializeWrapper::new(
        ws::ClientMessage::<()>::Subscribe(ws::Subscribe {
            query_strings,
            request_id: 0,
        })
    )).unwrap();
    ws.send(msg.into()).await
}

/// Await the initial [`ServerMessage::SubscriptionUpdate`].
/// If `module_def` is `Some`, print a JSON representation to stdout.
async fn await_initial_update<S>(ws: &mut S, module_def: Option<&ModuleDef>) -> anyhow::Result<()>
where
    S: TryStream<Ok = WsMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    while let Some(msg) = ws.try_next().await.unwrap() {
        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws::ServerMessage::InitialSubscription(sub) => {
                if let Some(module_def) = module_def {
                    let formatted = reformat_update(sub.database_update, module_def).unwrap();
                    let output = serde_json::to_string(&formatted).unwrap() + "\n";
                    tokio::io::stdout().write_all(output.as_bytes()).await.unwrap()
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
async fn consume_transaction_updates<S>(ws: &mut S, num: Option<u32>, module_def: &ModuleDef) -> anyhow::Result<bool>
where
    S: TryStream<Ok = WsMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let mut stdout = tokio::io::stdout();
    let mut num_received = 0;
    loop {
        if num.is_some_and(|n| num_received >= n) {
            break Ok(true);
        }
        let Some(msg) = ws.try_next().await.unwrap() else {
            eprintln!("disconnected by server");
            break Ok(false);
        };

        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws::ServerMessage::InitialSubscription(_) => {
                anyhow::bail!("protocol error: received a second initial subscription update")
            }
            ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate {
                status: ws::UpdateStatus::Committed(update), ..
            }) => {
                let output = serde_json::to_string(&reformat_update(update, module_def).unwrap()).unwrap() + "\n";
                stdout.write_all(output.as_bytes()).await.unwrap();
                num_received += 1;
            }
            _ => continue,
        }
    }
}
