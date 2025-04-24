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
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::api::ClientApi;
use crate::common_args;
use crate::sql::parse_req;
use crate::util::UNSTABLE_WARNING;
use crate::Config;
use std::path::{Path, PathBuf};

pub fn cli() -> clap::Command {
    clap::Command::new("subscribe")
        .about(format!(
            "Subscribe to SQL queries on the database. {}",
            UNSTABLE_WARNING
        ))
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
        //.arg(common_args::cert())
        .arg(common_args::trust_server_cert())
        .arg(common_args::client_cert())
        .arg(common_args::client_key())
        .arg(common_args::trust_system_root_store())
        .arg(common_args::no_trust_system_root_store())
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
    eprintln!("{}\n", UNSTABLE_WARNING);

    let queries = args.get_many::<String>("query").unwrap();
    let num = args.get_one::<u32>("num-updates").copied();
    let timeout = args.get_one::<u32>("timeout").copied();
    let print_initial_update = args.get_flag("print_initial_update");

    // TLS arguments
    let trust_server_cert_path: Option<&Path> = args.get_one::<PathBuf>("trust-server-cert").map(|p| p.as_path());
    let client_cert_path: Option<&Path> = args.get_one::<PathBuf>("client-cert").map(|p| p.as_path());
    let client_key_path: Option<&Path> = args.get_one::<PathBuf>("client-key").map(|p| p.as_path());

    // for clients, default to true unless --no-trust-system-root-store
    // because this is used to verify the received server cert which can be signed by public CA
    // thus using system's trust/root store, by default, makes sense.
    let trust_system = !args.get_flag("no-trust-system-root-store");

    let conn = parse_req(config, args,
        trust_server_cert_path,
        client_cert_path,
        client_key_path,
        trust_system,
        ).await?;
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

    // Configure TLS with cert_path
    let connector = if req.uri().scheme_str() != Some("wss") {
        let b:bool=trust_server_cert_path.is_some() || client_cert_path.is_some() || client_key_path.is_some();
        if b {
            return Err(anyhow::anyhow!("Using cert(s)/key require using https:// scheme not http://"));
        }
        None
    } else {
        let mut builder = native_tls::TlsConnector::builder();
        
        // Validate trust store
        if !trust_system && trust_server_cert_path.is_none() {
            return Err(anyhow::anyhow!(
                "--no-trust-system-root-store requires --trust-server-cert"
            ));
        }
        if !trust_system {
            builder.disable_built_in_roots(true);
        }

        if let Some(cert_path) = trust_server_cert_path {
            let cert_data = spacetimedb_lib::read_file_limited(cert_path)
                .await
                .context(format!("Failed to read cert file: {}", cert_path.display()))?;
            let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_data))
                .collect::<Result<Vec<_>, _>>()
                .context(format!("Failed to parse trust certificates: {}", cert_path.display()))?;
            if certs.is_empty() {
                return Err(anyhow::anyhow!(
                    "No valid certificates in: {}",
                    cert_path.display()
                ));
            }
            for cert in certs {
                //TODO: show me added certs like we do in other places!
                let native_cert = native_tls::Certificate::from_der(&cert).context(format!(
                    "Failed to convert cert to native-tls format from {}",
                    cert_path.display()
                ))?;
                builder.add_root_certificate(native_cert);
            }
        }

        // Configure mTLS
        if let Some(cert_path) = client_cert_path {
            let key_path = client_key_path.ok_or_else(|| {
                anyhow::anyhow!("--client-key is required with --client-cert")
            })?;
            let cert_data = spacetimedb_lib::read_file_limited(cert_path)
                .await
                .context(format!("Failed to read client cert: {}", cert_path.display()))?;
            let key_data = spacetimedb_lib::read_file_limited(key_path)
                .await
                .context(format!("Failed to read client key: {}", key_path.display()))?;
            let identity = native_tls::Identity::from_pkcs8(&cert_data, &key_data).context(format!(
                "Failed to parse client cert/key: {}",
                cert_path.display()
            ))?;
            builder.identity(identity);
        }

        let tls_connector = builder.build().context("Failed to build TLS connector")?;
        Some(tokio_tungstenite::Connector::NativeTls(tls_connector))
    };

    let (mut ws, _) = tokio_tungstenite::connect_async_tls_with_config(req, None, false, connector).await?;

    let task = async {
        subscribe(&mut ws, queries.cloned().map(Into::into).collect()).await?;
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
async fn subscribe<S>(ws: &mut S, query_strings: Box<[Box<str>]>) -> Result<(), S::Error>
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
/// If `module_def` is `Some`, print a JSON representation to stdout.
async fn await_initial_update<S>(ws: &mut S, module_def: Option<&RawModuleDefV9>) -> anyhow::Result<()>
where
    S: TryStream<Ok = WsMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    const RECV_TX_UPDATE: &str = "protocol error: received transaction update before initial subscription update";

    while let Some(msg) = ws.try_next().await? {
        let Some(msg) = parse_msg_json(&msg) else { continue };
        match msg {
            ws::ServerMessage::InitialSubscription(sub) => {
                if let Some(module_def) = module_def {
                    let formatted = reformat_update(&sub.database_update, module_def)?;
                    let output = serde_json::to_string(&formatted)? + "\n";
                    tokio::io::stdout().write_all(output.as_bytes()).await?
                }
                break;
            }
            ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate { status, .. }) => anyhow::bail!(match status {
                ws::UpdateStatus::Failed(msg) => msg,
                _ => RECV_TX_UPDATE.into(),
            }),
            ws::ServerMessage::TransactionUpdateLight(ws::TransactionUpdateLight { .. }) => {
                anyhow::bail!(RECV_TX_UPDATE)
            }
            _ => continue,
        }
    }

    Ok(())
}

/// Print `num` [`ServerMessage::TransactionUpdate`] messages as JSON.
/// If `num` is `None`, keep going indefinitely.
async fn consume_transaction_updates<S>(
    ws: &mut S,
    num: Option<u32>,
    module_def: &RawModuleDefV9,
) -> anyhow::Result<bool>
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
            ws::ServerMessage::InitialSubscription(_) => {
                anyhow::bail!("protocol error: received a second initial subscription update")
            }
            ws::ServerMessage::TransactionUpdateLight(ws::TransactionUpdateLight { update, .. })
            | ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate {
                status: ws::UpdateStatus::Committed(update),
                ..
            }) => {
                let output = serde_json::to_string(&reformat_update(&update, module_def)?)? + "\n";
                stdout.write_all(output.as_bytes()).await?;
                num_received += 1;
            }
            _ => continue,
        }
    }
}
