use std::time::Duration; 
 
use anyhow::Context; 
use clap::{value_parser, Arg, ArgAction, ArgMatches}; 
use futures::{Sink, SinkExt, TryStream, TryStreamExt}; 
use http::header; 
use http::uri::Scheme; 
use serde_json::value::RawValue; 
use spacetimedb_data_structures::map::HashMap; 
use spacetimedb_lib::{sats, ModuleDef}; 
use spacetimedb_standalone::TEXT_PROTOCOL; 
use tokio::io::AsyncWriteExt; 
use tokio_tungstenite::tungstenite::client::IntoClientRequest; 
use tokio_tungstenite::tungstenite::Message as WsMessage; 
 
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
    fn reformat(self, schema: &ModuleDef) -> anyhow::Result<HashMap<String, SubscriptionTable>> { 
        self.table_updates 
            .into_iter() 
            .map(|upd| { 
                let table_schema = schema 
                    .tables 
                    .iter() 
                    .find(|tbl| tbl.schema.table_name == upd.table_name) 
                    .context("table not found in schema")?; 
                let mut deletes = Vec::new(); 
                let mut inserts = Vec::new(); 
                let table_ty = schema.typespace.resolve(table_schema.data); 
                for op in upd.table_row_operations { 
                    let row = serde::de::DeserializeSeed::deserialize(sats::de::serde::SeedWrapper(table_ty), op.row)?; 
                    let row = table_ty.with_value(&row); 
                    let row = serde_json::to_value(sats::ser::serde::SerializeWrapper::from_ref(&row))?; 
                    match op.op { 
                        Op::Delete => deletes.push(row), 
                        Op::Insert => inserts.push(row), 
                    } 
                } 
                Ok((upd.table_name.into(), SubscriptionTable { deletes, inserts })) 
            }) 
            .collect() 
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
        await_initial_update(&mut ws, print_initial_update.then_some(&module_def)).await?; 
        consume_transaction_updates(&mut ws, num, &module_def).await 
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
/// If `module_def` is `Some`, print a JSON representation to stdout. 
async fn await_initial_update<S>(ws: &mut S, module_def: Option<&ModuleDef>) -> anyhow::Result<()> 
where 
    S: TryStream<Ok = WsMessage> + Unpin, 
    S::Error: std::error::Error + Send + Sync + 'static, 
{ 
    while let Some(msg) = ws.try_next().await? { 
        let Some(msg) = parse_msg_json(&msg) else { continue }; 
        match msg { 
            ServerMessage::SubscriptionUpdate(sub) => { 
                if let Some(module_def) = module_def { 
                    let output = serde_json::to_string(&sub.reformat(module_def)?)? + "\n"; 
                    tokio::io::stdout().write_all(output.as_bytes()).await? 
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
                let output = serde_json::to_string(&subscription_update.reformat(module_def)?)? + "\n"; 
                stdout.write_all(output.as_bytes()).await?; 
                num_received += 1; 
            } 
            ServerMessage::Unknown => continue, 
        } 
    } 
} 
