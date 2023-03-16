use crate::client::ClientActorId;
use crate::host::host_controller;
use crate::host::ReducerArgs;
use crate::protobuf::client_api::Subscribe;
use crate::protobuf::client_api::{message, Message};
use crate::worker_metrics::{WEBSOCKET_REQUESTS, WEBSOCKET_REQUEST_MSG_SIZE};
use prost::bytes::Bytes;
use prost::Message as OtherMessage;

pub async fn handle_binary(
    client_id: ClientActorId,
    instance_id: u64,
    message_buf: Vec<u8>,
) -> Result<(), anyhow::Error> {
    WEBSOCKET_REQUEST_MSG_SIZE
        .with_label_values(&[format!("{}", instance_id).as_str(), "binary"])
        .observe(message_buf.len() as f64);

    WEBSOCKET_REQUESTS
        .with_label_values(&[format!("{}", instance_id).as_str(), "binary"])
        .inc();

    let message = Message::decode(Bytes::from(message_buf))?;
    match message.r#type {
        Some(message::Type::FunctionCall(f)) => {
            let reducer = f.reducer;
            let args = ReducerArgs::Json(f.arg_bytes.into());

            let host = host_controller::get();
            match host.call_reducer(instance_id, client_id.identity, &reducer, args).await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("{:#}", e)
                }
            }

            Ok(())
        }
        Some(message::Type::Subscribe(subscribe)) => {
            let host = host_controller::get();
            let module = host.get_module(instance_id);
            match module {
                Ok(module) => {
                    module.add_subscriber(client_id, subscribe).await?;
                }
                Err(e) => {
                    log::warn!("Could not find module {} to subscribe to: {:?}", instance_id, e)
                }
            }
            Ok(())
        }
        Some(_) => Err(anyhow::anyhow!("Unexpected client message type.")),
        None => Err(anyhow::anyhow!("No message from client")),
    }
}

pub async fn handle_text(client_id: ClientActorId, instance_id: u64, message: String) -> Result<(), anyhow::Error> {
    WEBSOCKET_REQUEST_MSG_SIZE
        .with_label_values(&[format!("{}", instance_id).as_str(), "text"])
        .observe(message.len() as f64);

    WEBSOCKET_REQUESTS
        .with_label_values(&[format!("{}", instance_id).as_str(), "text"])
        .inc();

    #[derive(serde::Deserialize)]
    enum Message<'a> {
        #[serde(rename = "call")]
        Call {
            #[serde(borrow, rename = "fn")]
            func: std::borrow::Cow<'a, str>,
            args: &'a serde_json::value::RawValue,
        },
        #[serde(rename = "subscribe")]
        Subscribe { query_strings: Vec<String> },
    }

    let bytes = Bytes::from(message);
    match serde_json::from_slice::<Message>(&bytes)? {
        Message::Call { func, args } => {
            let args = ReducerArgs::Json(bytes.slice_ref(args.get().as_bytes()));

            let host = host_controller::get();
            match host.call_reducer(instance_id, client_id.identity, &func, args).await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("{:#}", e)
                }
            }
        }
        Message::Subscribe { query_strings } => {
            let host = host_controller::get();
            let module = host.get_module(instance_id);
            match module {
                Ok(module) => {
                    module.add_subscriber(client_id, Subscribe { query_strings }).await?;
                }
                Err(e) => {
                    log::warn!("Could not find module {} to subscribe to: {:?}", instance_id, e)
                }
            }
        }
    }

    Ok(())
}
