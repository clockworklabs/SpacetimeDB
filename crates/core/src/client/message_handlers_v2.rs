use crate::client::MessageExecutionError;

use super::{ClientConnection, DataMessage, MessageHandleError};
use core::panic;
use serde::de::Error as _;
use spacetimedb_client_api_messages::websocket::v2 as ws_v2;
use spacetimedb_datastore::execution_context::WorkloadType;
use spacetimedb_lib::bsatn;
use spacetimedb_primitives::ReducerId;
use std::time::Instant;

pub async fn handle(client: &ClientConnection, message: DataMessage, timer: Instant) -> Result<(), MessageHandleError> {
    client.observe_websocket_request_message(&message);
    let message = match message {
        DataMessage::Binary(message_buf) => bsatn::from_slice::<ws_v2::ClientMessage>(&message_buf)?,
        DataMessage::Text(_) => {
            return Err(MessageHandleError::TextDecode(serde_json::Error::custom(
                "v2 websocket does not support text messages",
            )))
        }
    };
    let module = client.module();
    let mod_info = module.info();
    let mod_metrics = &mod_info.metrics;
    let _database_identity = mod_info.database_identity;
    let db = &module.replica_ctx().relational_db;
    let record_metrics = |wl| {
        move |metrics| {
            if let Some(metrics) = metrics {
                db.exec_counters_for(wl).record(&metrics);
            }
        }
    };
    let sub_metrics = record_metrics(WorkloadType::Subscribe);
    let res: Result<(), (Option<&Box<str>>, Option<ReducerId>, anyhow::Error)> = match message {
        ws_v2::ClientMessage::Subscribe(subscribe) => {
            let res = client.subscribe_v2(subscribe, timer).await.map(sub_metrics);
            mod_metrics
                .request_round_trip_subscribe
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ws_v2::ClientMessage::Unsubscribe(_) => panic!("v2 unsubscribe not implemented yet"),
        ws_v2::ClientMessage::OneOffQuery(_) => panic!("v2 one-off query not implemented yet"),
        ws_v2::ClientMessage::CallReducer(_) => panic!("v2 call reducer not implemented yet"),
        ws_v2::ClientMessage::CallProcedure(_) => panic!("v2 call procedure not implemented yet"),
    };
    res.map_err(|(reducer_name, reducer_id, err)| MessageExecutionError {
        reducer: reducer_name.cloned(),
        reducer_id,
        caller_identity: client.id.identity,
        caller_connection_id: Some(client.id.connection_id),
        err,
    })?;

    Ok(())
}
