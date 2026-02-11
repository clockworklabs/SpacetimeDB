use crate::client::MessageExecutionError;

use super::{ClientConnection, DataMessage, MessageHandleError};
use crate::worker_metrics::WORKER_METRICS;
use serde::de::Error as _;
use spacetimedb_client_api_messages::websocket::v2 as ws_v2;
use spacetimedb_datastore::execution_context::WorkloadType;
use spacetimedb_lib::{bsatn, Timestamp};
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
    let database_identity = mod_info.database_identity;
    let db = &module.replica_ctx().relational_db;
    let record_metrics = |wl| {
        move |metrics| {
            if let Some(metrics) = metrics {
                db.exec_counters_for(wl).record(&metrics);
            }
        }
    };
    let sub_metrics = record_metrics(WorkloadType::Subscribe);
    let unsub_metrics = record_metrics(WorkloadType::Unsubscribe);
    type HandleResult<'a> = Result<(), (Option<&'a Box<str>>, Option<ReducerId>, anyhow::Error)>;
    let res: HandleResult<'_> = match message {
        ws_v2::ClientMessage::Subscribe(subscribe) => {
            let res = client.subscribe_v2(subscribe, timer).await.map(sub_metrics);
            mod_metrics
                .request_round_trip_subscribe
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ws_v2::ClientMessage::Unsubscribe(unsubscribe) => {
            let res = client.unsubscribe_v2(unsubscribe, timer).await.map(unsub_metrics);
            mod_metrics
                .request_round_trip_unsubscribe
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ws_v2::ClientMessage::OneOffQuery(ws_v2::OneOffQuery {
            request_id,
            query_string,
        }) => {
            let res = client.one_off_query_v2(&query_string, request_id, timer).await;
            mod_metrics
                .request_round_trip_sql
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|err| (None, None, err))
        }
        ws_v2::ClientMessage::CallReducer(ws_v2::CallReducer {
            ref reducer,
            args,
            request_id,
            flags,
        }) => {
            let res = client.call_reducer_v2(reducer, args, request_id, timer, flags).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Reducer, &database_identity, reducer)
                .observe(timer.elapsed().as_secs_f64());
            match res {
                Ok(_) => {
                    // If this was not a success, we would have already sent an error message.
                    Ok(())
                }
                Err(e) => {
                    let err_msg = format!("{e:#}");
                    let server_message = ws_v2::ServerMessage::ReducerResult(ws_v2::ReducerResult {
                        request_id,
                        // Maybe we should use the same timestamp that was used for the reducer context, but this is probably fine for now.
                        timestamp: Timestamp::now(),
                        result: ws_v2::ReducerOutcome::InternalError(err_msg.into()),
                    });
                    // TODO: Should we kill the client here, or does it mean the client is already dead.
                    if let Err(send_err) = client.send_message(None, server_message) {
                        log::warn!("Failed to send reducer error to client: {send_err}");
                    }
                    Ok(())
                }
            }
        }
        ws_v2::ClientMessage::CallProcedure(ws_v2::CallProcedure {
            ref procedure,
            args,
            request_id,
            flags,
        }) => {
            let res = client
                .call_procedure_v2(procedure, args, request_id, timer, flags)
                .await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Procedure, &database_identity, procedure)
                .observe(timer.elapsed().as_secs_f64());
            if let Err(e) = res {
                log::warn!("Procedure call failed: {e:#}");
            }
            Ok(())
        }
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
