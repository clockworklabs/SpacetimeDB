use anyhow::Context;
use bytes::Bytes;
use clap::{value_parser, Arg, ArgAction, ArgMatches};
use futures::{Sink, SinkExt, TryStream, TryStreamExt};
use http::header;
use reqwest::Url;
use serde_json::Value;
use spacetimedb_client_api_messages::websocket::{common as ws_common, v1 as ws_v1, v2 as ws_v2, v3 as ws_v3};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::de::serde::{DeserializeWrapper, SeedWrapper};
use spacetimedb_lib::de::DeserializeSeed as BsatnDeserializeSeed;
use spacetimedb_lib::sats::WithTypespace;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::{bsatn, AlgebraicType};
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::AtomicU32;
use std::time::Duration;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request as WsRequest;
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::api::ClientApi;
use crate::common_args;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_optional_database_parts};
use crate::util::UNSTABLE_WARNING;
use crate::util::{database_identity, get_auth_header};
use crate::Config;

pub fn cli() -> clap::Command {
    clap::Command::new("subscribe")
        .about(format!("Subscribe to SQL queries on the database. {UNSTABLE_WARNING}"))
        .arg(
            Arg::new("subscribe_parts")
                .num_args(1..)
                .help("Subscribe arguments: [DATABASE] <QUERY> [QUERY...]"),
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
                     subscription messages. If `-n` is specified, it will stop after whichever \
                     one comes first. Timing out before receiving `-n` updates is an error.",
                ),
        )
        .arg(
            Arg::new("print_initial_update")
                .required(false)
                .long("print-initial-update")
                .action(ArgAction::SetTrue)
                .help("Print the initial update for the queries."),
        )
        .arg(common_args::confirmed())
        .arg(common_args::anonymous())
        .arg(common_args::yes())
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(ArgAction::SetTrue)
                .help("Ignore spacetime.json configuration"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
}

#[derive(serde::Serialize, Debug)]
struct SubscriptionTable {
    deletes: Vec<serde_json::Value>,
    inserts: Vec<serde_json::Value>,
}

/// Concrete websocket stream type returned by `tokio_tungstenite::connect_async`.
type SubscribeWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Active websocket connection for `spacetime subscribe`.
///
/// The command prefers the v3 transport so smoketests and normal CLI usage
/// exercise the coalesced server path, but it keeps the old v1 text transport
/// as a fallback for older servers.
enum SubscribeConnection {
    /// v3 uses BSATN-encoded v2 messages, possibly coalesced in one websocket payload.
    V3 {
        ws: SubscribeWebSocket,
        /// Decoded messages left over from a coalesced v3 websocket payload.
        pending: VecDeque<ws_v2::ServerMessage>,
    },
    /// v1 is the historical JSON text protocol.
    V1 { ws: SubscribeWebSocket },
}

impl SubscribeConnection {
    /// Send the subscribe request using whichever protocol was negotiated.
    async fn subscribe(&mut self, query_strings: Box<[Box<str>]>) -> Result<(), Error> {
        match self {
            Self::V3 { ws, .. } => subscribe_v3(ws, query_strings).await,
            Self::V1 { ws } => subscribe_v1(ws, query_strings).await,
        }
    }

    /// Wait for the initial subscription result and optionally print it.
    async fn await_initial_update(&mut self, module_def: Option<&RawModuleDefV9>) -> Result<(), Error> {
        match self {
            Self::V3 { ws, pending } => await_initial_update_v3(ws, pending, module_def).await,
            Self::V1 { ws } => await_initial_update_v1(ws, module_def).await,
        }
    }

    /// Print transaction updates until the requested count is reached.
    async fn consume_transaction_updates(
        &mut self,
        num: Option<u32>,
        module_def: &RawModuleDefV9,
        num_received: &UpdateCounter,
    ) -> Result<(), Error> {
        match self {
            Self::V3 { ws, pending } => {
                consume_transaction_updates_v3(ws, pending, num, module_def, num_received).await
            }
            Self::V1 { ws } => consume_transaction_updates_v1(ws, num, module_def, num_received).await,
        }
    }

    /// Best-effort graceful websocket close.
    async fn close(&mut self) {
        match self {
            Self::V3 { ws, .. } | Self::V1 { ws } => {
                let _ = ws.close(None).await;
            }
        }
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{UNSTABLE_WARNING}\n");

    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    let anon_identity = args.get_flag("anon_identity");
    let no_config = args.get_flag("no_config");

    let raw_parts: Vec<String> = args
        .get_many::<String>("subscribe_parts")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_optional_database_parts(
        &raw_parts,
        config_targets.as_deref(),
        "query",
        "spacetime subscribe [database] <query> [query...] (or --no-config for legacy behavior)",
    )?;
    let queries: Vec<String> = resolved.remaining_args;

    let num = args.get_one::<u32>("num-updates").copied();
    let timeout = args.get_one::<u32>("timeout").copied();
    let print_initial_update = args.get_flag("print_initial_update");
    let confirmed = args.get_one::<bool>("confirmed").copied();
    let resolved_server = server.or(resolved.server.as_deref());

    let mut config = config;
    let conn = crate::api::Connection {
        host: config.get_host_url(resolved_server)?,
        auth_header: get_auth_header(&mut config, anon_identity, resolved_server, !force).await?,
        database_identity: database_identity(&config, &resolved.database, resolved_server).await?,
        database: resolved.database.clone(),
    };
    let api = ClientApi::new(conn);
    let module_def = api.module_def().await?;
    let mut conn = connect_with_fallback(&api, confirmed).await?;
    let num_received = UpdateCounter::new();

    let task = async {
        conn.subscribe(queries.iter().cloned().map(Into::into).collect())
            .await?;
        conn.await_initial_update(print_initial_update.then_some(&module_def))
            .await?;
        conn.consume_transaction_updates(num, &module_def, &num_received).await
    };

    let res = if let Some(timeout) = timeout {
        let timeout = Duration::from_secs(timeout.into());
        match tokio::time::timeout(timeout, task).await {
            Ok(res) => res,
            Err(_elapsed) => {
                let received = num_received.get();
                eprintln!("timed out after {}s", timeout.as_secs());
                match num {
                    Some(expected) if received < expected => Err(Error::UpdateLimitTimedOut {
                        expected,
                        received,
                        timeout_secs: timeout.as_secs(),
                    }),
                    _ => Ok(()),
                }
            }
        }
    } else {
        task.await
    };

    // Close the connection gracefully.
    // This will return an error if the server already closed,
    // or the connection is in a bad state.
    // The error (if any) relevant to the user is already stored in `res`.
    conn.close().await;
    // The server closing the connection is not considered an error
    // if `-n` is not set, but any other error is. When `-n` is set,
    // an early close or timeout from the update loop is reported as an error.
    res.or_else(|e| {
        if e.is_server_closed_connection() {
            Ok(())
        } else {
            Err(e)
        }
    })
    .map_err(anyhow::Error::from)
}

/// Connect using v3 when available, otherwise retry once with the v1 text protocol.
///
/// Fallback is intentionally limited to connection setup and protocol
/// negotiation. After a v3 connection is accepted, malformed v3 data is a real
/// error and should not be hidden by silently reconnecting with v1.
async fn connect_with_fallback(api: &ClientApi, confirmed: Option<bool>) -> Result<SubscribeConnection, anyhow::Error> {
    match connect_v3(api, confirmed).await {
        Ok(conn) => Ok(conn),
        Err(v3_error) => connect_v1(api, confirmed)
            .await
            .with_context(|| format!("v3 subscribe connection failed ({v3_error}); v1 fallback also failed")),
    }
}

/// Open a v3 subscribe websocket and validate that the server negotiated v3.
async fn connect_v3(api: &ClientApi, confirmed: Option<bool>) -> Result<SubscribeConnection, anyhow::Error> {
    let req = subscribe_request(api, confirmed, ws_v3::BIN_PROTOCOL, true)?;
    let (ws, response) = tokio_tungstenite::connect_async(req).await?;
    if response
        .headers()
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        != Some(ws_v3::BIN_PROTOCOL)
    {
        return Err(Error::Protocol {
            details: "server did not negotiate the v3 websocket protocol",
        }
        .into());
    }
    Ok(SubscribeConnection::V3 {
        ws,
        pending: VecDeque::new(),
    })
}

/// Open a v1 text subscribe websocket for compatibility with older servers.
async fn connect_v1(api: &ClientApi, confirmed: Option<bool>) -> Result<SubscribeConnection, anyhow::Error> {
    let req = subscribe_request(api, confirmed, ws_v1::TEXT_PROTOCOL, false)?;
    let (ws, _) = tokio_tungstenite::connect_async(req).await?;
    Ok(SubscribeConnection::V1 { ws })
}

/// Build a subscribe websocket request for a specific subprotocol.
///
/// `request_uncompressed` is used only for the CLI v3 path. The CLI decodes
/// enough v3 to print JSON output, but does not implement brotli or gzip
/// decoding, so v3 asks the server for uncompressed payloads. The v1 fallback
/// leaves the query string in its historical shape.
fn subscribe_request(
    api: &ClientApi,
    confirmed: Option<bool>,
    protocol: &'static str,
    request_uncompressed: bool,
) -> Result<WsRequest, anyhow::Error> {
    let mut url = Url::parse(&api.con.db_uri("subscribe"))?;
    // Change the URI scheme from `http(s)` to `ws(s)`.
    url.set_scheme(match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        unknown => unreachable!("Invalid URL scheme in `Connection::db_uri`: {unknown}"),
    })
    .unwrap();
    {
        let mut query = url.query_pairs_mut();
        if request_uncompressed {
            // The CLI v3 path only needs enough support to print updates as
            // JSON, so request uncompressed payloads and avoid brotli/gzip
            // decoding here. The v1 fallback preserves the old URL shape.
            query.append_pair("compression", "None");
        }
        if let Some(confirmed) = confirmed {
            query.append_pair("confirmed", if confirmed { "true" } else { "false" });
        }
    }

    let mut req = url.into_client_request()?;
    req.headers_mut()
        .insert(header::SEC_WEBSOCKET_PROTOCOL, http::HeaderValue::from_static(protocol));
    if let Some(auth_header) = api.con.auth_header.to_header() {
        req.headers_mut().insert(header::AUTHORIZATION, auth_header);
    }
    Ok(req)
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
    #[error("encountered error in initial subscribe: {reason}")]
    SubscribeFailure { reason: Box<str> },
    #[error("subscription closed after receiving {received}/{expected} updates")]
    UpdateLimitNotReached { expected: u32, received: u32 },
    #[error("subscription timed out after {timeout_secs}s after receiving {received}/{expected} updates")]
    UpdateLimitTimedOut {
        expected: u32,
        received: u32,
        timeout_secs: u64,
    },
    #[error("error formatting response: {source:#}")]
    Reformat {
        #[source]
        source: anyhow::Error,
    },
    #[error("error encoding BSATN websocket message: {source}")]
    BsatnEncode {
        #[source]
        source: spacetimedb_lib::bsatn::EncodeError,
    },
    #[error("error decoding BSATN websocket message: {source}")]
    BsatnDecode {
        #[source]
        source: spacetimedb_lib::bsatn::DecodeError,
    },
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

struct UpdateCounter(AtomicU32);

impl UpdateCounter {
    fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    fn get(&self) -> u32 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn increment(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
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

fn connection_closed_error(num: Option<u32>, num_received: u32) -> Error {
    match num {
        // this is necessarily an error, because if we had received all the updates then we would have already closed the connection ourselves and would not have reached this point
        Some(expected) => Error::UpdateLimitNotReached {
            expected,
            received: num_received,
        },
        None => {
            eprintln!("disconnected by server");
            Error::Websocket {
                source: WsError::ConnectionClosed,
            }
        }
    }
}

/// Send a v1 JSON subscribe message.
async fn subscribe_v1<S>(ws: &mut S, query_strings: Box<[Box<str>]>) -> Result<(), Error>
where
    S: Sink<WsMessage, Error = WsError> + Unpin,
{
    let msg = serde_json::to_string(&SerializeWrapper::new(ws_v1::ClientMessage::<()>::Subscribe(
        ws_v1::Subscribe {
            query_strings,
            request_id: 0,
        },
    )))
    .unwrap();
    ws.send(msg.into()).await.map_err(|source| Error::Subscribe { source })
}

/// Send a v3 BSATN subscribe message.
async fn subscribe_v3<S>(ws: &mut S, query_strings: Box<[Box<str>]>) -> Result<(), Error>
where
    S: Sink<WsMessage, Error = WsError> + Unpin,
{
    let msg = ws_v2::ClientMessage::Subscribe(ws_v2::Subscribe {
        request_id: 0,
        query_set_id: ws_v2::QuerySetId::new(0),
        query_strings,
    });
    let msg = bsatn::to_vec(&msg).map_err(|source| Error::BsatnEncode { source })?;
    ws.send(WsMessage::Binary(msg.into()))
        .await
        .map_err(|source| Error::Subscribe { source })
}

/// Parse a v1 text websocket message as JSON.
fn parse_msg_json(msg: &WsMessage) -> Option<ws_v1::ServerMessage<ws_v1::JsonFormat>> {
    let WsMessage::Text(msg) = msg else { return None };
    serde_json::from_str::<DeserializeWrapper<ws_v1::ServerMessage<ws_v1::JsonFormat>>>(msg)
        .inspect_err(|e| eprintln!("couldn't parse message from server: {e}"))
        .map(|wrapper| wrapper.0)
        .ok()
}

/// Await the initial v1 [`ws_v1::ServerMessage::InitialSubscription`].
/// If `module_def` is `Some`, print a JSON representation to stdout.
async fn await_initial_update_v1<S>(ws: &mut S, module_def: Option<&RawModuleDefV9>) -> Result<(), Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    const RECV_TX_UPDATE: &str = "protocol error: received transaction update before initial subscription update";

    while let Some(msg) = ws.try_next().await.map_err(|source| Error::Websocket { source })? {
        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws_v1::ServerMessage::InitialSubscription(sub) => {
                if let Some(module_def) = module_def {
                    let output = format_output_json_v1(&sub.database_update, module_def)?;
                    tokio::io::stdout().write_all(output.as_bytes()).await?
                }
                break;
            }
            ws_v1::ServerMessage::SubscriptionError(error) => {
                return Err(Error::SubscribeFailure { reason: error.error });
            }
            ws_v1::ServerMessage::TransactionUpdate(ws_v1::TransactionUpdate { status, .. }) => {
                return Err(match status {
                    ws_v1::UpdateStatus::Failed(msg) => Error::TransactionFailure { reason: msg },
                    _ => Error::Protocol {
                        details: RECV_TX_UPDATE,
                    },
                })
            }
            ws_v1::ServerMessage::TransactionUpdateLight(ws_v1::TransactionUpdateLight { .. }) => {
                return Err(Error::Protocol {
                    details: RECV_TX_UPDATE,
                })
            }
            _ => continue,
        }
    }

    Ok(())
}

/// Await the initial [`ws_v2::ServerMessage::SubscribeApplied`].
/// If `module_def` is `Some`, print a JSON representation to stdout.
async fn await_initial_update_v3<S>(
    ws: &mut S,
    pending: &mut VecDeque<ws_v2::ServerMessage>,
    module_def: Option<&RawModuleDefV9>,
) -> Result<(), Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    const RECV_TX_UPDATE: &str = "received transaction update before initial subscription update";

    while let Some(msg) = next_server_message(ws, pending).await? {
        match msg {
            ws_v2::ServerMessage::SubscribeApplied(sub) => {
                if let Some(module_def) = module_def {
                    let output = format_output_json_query_rows(&sub.rows, module_def)?;
                    tokio::io::stdout().write_all(output.as_bytes()).await?
                }
                break;
            }
            ws_v2::ServerMessage::SubscriptionError(error) => {
                return Err(Error::SubscribeFailure { reason: error.error });
            }
            ws_v2::ServerMessage::TransactionUpdate(_) => {
                return Err(Error::Protocol {
                    details: RECV_TX_UPDATE,
                })
            }
            _ => continue,
        }
    }

    Ok(())
}

/// Print `num` v1 [`ws_v1::ServerMessage::TransactionUpdate`] messages as JSON.
/// If `num` is `None`, keep going indefinitely.
async fn consume_transaction_updates_v1<S>(
    ws: &mut S,
    num: Option<u32>,
    module_def: &RawModuleDefV9,
    num_received: &UpdateCounter,
) -> Result<(), Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    let mut stdout = tokio::io::stdout();
    loop {
        if num.is_some_and(|n| num_received.get() >= n) {
            return Ok(());
        }
        let Some(msg) = ws.try_next().await.map_err(|source| Error::Websocket { source })? else {
            return Err(connection_closed_error(num, num_received.get()));
        };

        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws_v1::ServerMessage::InitialSubscription(_) => {
                return Err(Error::Protocol {
                    details: "received a second initial subscription update",
                })
            }
            ws_v1::ServerMessage::SubscriptionError(error) => {
                return Err(Error::SubscribeFailure { reason: error.error });
            }
            ws_v1::ServerMessage::TransactionUpdateLight(ws_v1::TransactionUpdateLight { update, .. })
            | ws_v1::ServerMessage::TransactionUpdate(ws_v1::TransactionUpdate {
                status: ws_v1::UpdateStatus::Committed(update),
                ..
            }) => {
                let output = format_output_json_v1(&update, module_def)?;
                stdout.write_all(output.as_bytes()).await?;
                num_received.increment();
            }
            _ => continue,
        }
    }
}

/// Print `num` [`ws_v2::ServerMessage::TransactionUpdate`] messages as JSON.
/// If `num` is `None`, keep going indefinitely.
async fn consume_transaction_updates_v3<S>(
    ws: &mut S,
    pending: &mut VecDeque<ws_v2::ServerMessage>,
    num: Option<u32>,
    module_def: &RawModuleDefV9,
    num_received: &UpdateCounter,
) -> Result<(), Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    let mut stdout = tokio::io::stdout();
    loop {
        if num.is_some_and(|n| num_received.get() >= n) {
            return Ok(());
        }
        let Some(msg) = next_server_message(ws, pending).await? else {
            return Err(connection_closed_error(num, num_received.get()));
        };

        match msg {
            ws_v2::ServerMessage::SubscribeApplied(_) => {
                return Err(Error::Protocol {
                    details: "received a second initial subscription update",
                })
            }
            ws_v2::ServerMessage::SubscriptionError(error) => {
                return Err(Error::SubscribeFailure { reason: error.error });
            }
            ws_v2::ServerMessage::TransactionUpdate(update) => {
                let output = format_output_json_transaction_update(&update, module_def)?;
                stdout.write_all(output.as_bytes()).await?;
                num_received.increment();
            }
            _ => continue,
        }
    }
}

/// Return the next decoded server message from a v3 websocket stream.
///
/// A v3 websocket payload can contain multiple consecutive BSATN-encoded v2
/// server messages, so decoded surplus messages are queued for the next call.
/// Non-binary messages are ignored because v3 server data is binary-only.
async fn next_server_message<S>(
    ws: &mut S,
    pending: &mut VecDeque<ws_v2::ServerMessage>,
) -> Result<Option<ws_v2::ServerMessage>, Error>
where
    S: TryStream<Ok = WsMessage, Error = WsError> + Unpin,
{
    loop {
        if let Some(msg) = pending.pop_front() {
            return Ok(Some(msg));
        }

        let Some(msg) = ws.try_next().await.map_err(|source| Error::Websocket { source })? else {
            return Ok(None);
        };
        let WsMessage::Binary(msg) = msg else { continue };
        decode_server_payload(msg, pending)?;
    }
}

/// Decode one uncompressed v3 websocket payload into queued v2 server messages.
///
/// The server prefixes each binary payload with a compression tag. This CLI path
/// requests `compression=None`, so any compressed tag is treated as a protocol
/// error rather than decoded here.
fn decode_server_payload(msg: Bytes, pending: &mut VecDeque<ws_v2::ServerMessage>) -> Result<(), Error> {
    let Some((&tag, mut remaining)) = msg.as_ref().split_first() else {
        return Err(Error::Protocol {
            details: "received empty v3 websocket payload",
        });
    };
    if tag != ws_common::SERVER_MSG_COMPRESSION_TAG_NONE {
        return Err(Error::Protocol {
            details: "compressed v3 subscribe payload is not supported by this CLI path",
        });
    }
    if remaining.is_empty() {
        return Err(Error::Protocol {
            details: "received v3 websocket payload without a server message",
        });
    }

    while !remaining.is_empty() {
        let msg = bsatn::from_reader(&mut remaining).map_err(|source| Error::BsatnDecode { source })?;
        pending.push_back(msg);
    }

    Ok(())
}

/// Format a v1 database update using the legacy JSON row representation.
fn format_output_json_v1(
    msg: &ws_v1::DatabaseUpdate<ws_v1::JsonFormat>,
    schema: &RawModuleDefV9,
) -> Result<String, Error> {
    let formatted = reformat_update_v1(msg, schema).map_err(|source| Error::Reformat { source })?;
    format_output_json_from_tables(&formatted)
}

/// Format initial v3 subscription rows using the CLI's existing JSON output shape.
fn format_output_json_query_rows(msg: &ws_v2::QueryRows, schema: &RawModuleDefV9) -> Result<String, Error> {
    let formatted = reformat_query_rows(msg, schema).map_err(|source| Error::Reformat { source })?;
    format_output_json_from_tables(&formatted)
}

/// Format a v3 transaction update using the CLI's existing JSON output shape.
fn format_output_json_transaction_update(
    msg: &ws_v2::TransactionUpdate,
    schema: &RawModuleDefV9,
) -> Result<String, Error> {
    let formatted = reformat_transaction_update(msg, schema).map_err(|source| Error::Reformat { source })?;
    format_output_json_from_tables(&formatted)
}

/// Serialize the normalized table update map as one JSON object per output line.
fn format_output_json_from_tables(formatted: &HashMap<&str, SubscriptionTable>) -> Result<String, Error> {
    let output = serde_json::to_string(formatted)? + "\n";
    Ok(output)
}

/// Convert a v1 JSON-format database update to the normalized table output map.
fn reformat_update_v1<'a>(
    msg: &'a ws_v1::DatabaseUpdate<ws_v1::JsonFormat>,
    schema: &RawModuleDefV9,
) -> anyhow::Result<HashMap<&'a str, SubscriptionTable>> {
    msg.tables
        .iter()
        .map(|upd| {
            let table_ty = schema.typespace.resolve(
                schema
                    .type_ref_for_table_like(&upd.table_name)
                    .context("table not found in schema")?,
            );

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

/// Convert v3 initial subscription rows to the normalized table output map.
fn reformat_query_rows<'a>(
    msg: &'a ws_v2::QueryRows,
    schema: &RawModuleDefV9,
) -> anyhow::Result<HashMap<&'a str, SubscriptionTable>> {
    let mut formatted = HashMap::default();

    for table in &msg.tables {
        let table_ty = schema.typespace.resolve(
            schema
                .type_ref_for_table_like(&table.table)
                .context("table not found in schema")?,
        );
        let table_output = formatted.entry(&*table.table).or_insert_with(|| SubscriptionTable {
            deletes: Vec::new(),
            inserts: Vec::new(),
        });
        table_output.inserts.extend(reformat_bsatn_rows(&table.rows, table_ty)?);
    }

    Ok(formatted)
}

/// Convert a v3 transaction update to the normalized table output map.
fn reformat_transaction_update<'a>(
    msg: &'a ws_v2::TransactionUpdate,
    schema: &RawModuleDefV9,
) -> anyhow::Result<HashMap<&'a str, SubscriptionTable>> {
    let mut formatted = HashMap::default();

    for query_set in &msg.query_sets {
        for table in &query_set.tables {
            let table_ty = schema.typespace.resolve(
                schema
                    .type_ref_for_table_like(&table.table_name)
                    .context("table not found in schema")?,
            );
            let table_output = formatted
                .entry(&*table.table_name)
                .or_insert_with(|| SubscriptionTable {
                    deletes: Vec::new(),
                    inserts: Vec::new(),
                });
            for rows in &table.rows {
                match rows {
                    ws_v2::TableUpdateRows::PersistentTable(rows) => {
                        table_output
                            .deletes
                            .extend(reformat_bsatn_rows(&rows.deletes, table_ty)?);
                        table_output
                            .inserts
                            .extend(reformat_bsatn_rows(&rows.inserts, table_ty)?);
                    }
                    ws_v2::TableUpdateRows::EventTable(rows) => {
                        table_output
                            .inserts
                            .extend(reformat_bsatn_rows(&rows.events, table_ty)?);
                    }
                }
            }
        }
    }

    Ok(formatted)
}

/// Decode BSATN row-list entries and re-encode them as schema-aware JSON values.
fn reformat_bsatn_rows(
    rows: &ws_common::BsatnRowList,
    table_ty: WithTypespace<'_, AlgebraicType>,
) -> anyhow::Result<Vec<Value>> {
    rows.into_iter()
        .map(|row| {
            let mut row = row.as_ref();
            let row = BsatnDeserializeSeed::deserialize(table_ty, bsatn::Deserializer::new(&mut row))?;
            let row = table_ty.with_value(&row);
            Ok(serde_json::to_value(SerializeWrapper::from_ref(&row))?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::sats::{
        algebraic_value::de::ValueDeserializer, de::Deserialize, GroundSpacetimeType, Typespace, WithTypespace,
    };
    use spacetimedb_lib::ConnectionId;

    #[test]
    fn serde_json_value_preserves_connection_id_u128() -> anyhow::Result<()> {
        // `spacetime subscribe` reformats JSON rows through `serde_json::Value`
        // before typed SATS deserialization. The CLI enables
        // `serde_json/arbitrary_precision` so large `ConnectionId` values do not
        // get rounded while inside `Value`.
        let conn_id = ConnectionId::from_u128(u64::MAX as u128 + 1);
        let json = serde_json::to_string(&SerializeWrapper::new(&conn_id))?;
        let row = serde_json::from_str::<Value>(&json)?;

        let typespace = Typespace::default();
        let conn_id_ty = ConnectionId::get_type();
        let conn_id_ty = WithTypespace::new(&typespace, &conn_id_ty);
        let de = serde::de::DeserializeSeed::deserialize(SeedWrapper(conn_id_ty), row)?;
        let de = ConnectionId::deserialize(ValueDeserializer::new(de)).unwrap();

        assert_eq!(conn_id, de);
        Ok(())
    }
}
