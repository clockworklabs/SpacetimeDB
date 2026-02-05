use super::{ClientConnection, DataMessage};
use crate::host::FunctionArgs;
use crate::worker_metrics::WORKER_METRICS;
use spacetimedb_client_api_messages::websocket::v2::{self as ws_v2, ClientMessage};
use spacetimedb_datastore::execution_context::WorkloadType;
use spacetimedb_lib::bsatn;
use std::time::Instant;

use super::message_handlers::MessageHandleError;

pub async fn handle(client: &ClientConnection, message: DataMessage, timer: Instant) -> Result<(), MessageHandleError> {
    client.observe_websocket_request_message(&message);

    // V2 protocol is always binary (BSATN).
    let message_buf = match message {
        DataMessage::Binary(buf) => buf,
        DataMessage::Text(_) => {
            return Err(MessageHandleError::UnsupportedVersion(
                "v2 protocol does not support text messages",
            ));
        }
    };

    let message = bsatn::from_slice::<ClientMessage>(&message_buf)?;

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

    match message {
        ClientMessage::Subscribe(subscription) => {
            let res = client.subscribe_v2(subscription, timer).await.map(sub_metrics);
            mod_metrics
                .request_round_trip_subscribe
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| super::message_handlers_v1::MessageExecutionError {
                reducer: None,
                reducer_id: None,
                caller_identity: client.id.identity,
                caller_connection_id: Some(client.id.connection_id),
                err: e.into(),
            })?;
        }
        ClientMessage::Unsubscribe(_request) => {
            // TODO: Implement v2 unsubscribe once the method is available on ClientConnection.
            return Err(MessageHandleError::UnsupportedVersion(
                "v2 unsubscribe is not yet implemented",
            ));
        }
        ClientMessage::OneOffQuery(ws_v2::OneOffQuery {
            query_string: query,
            request_id: _,
        }) => {
            // V2 is always binary, so use the BSATN one-off query path.
            // The v2 OneOffQuery doesn't have a message_id field like v1;
            // use an empty slice as the message_id for compatibility.
            let res = client.one_off_query_bsatn(&query, &[], timer).await;
            mod_metrics
                .request_round_trip_sql
                .observe(timer.elapsed().as_secs_f64());
            if let Err(err) = res {
                return Err(super::message_handlers_v1::MessageExecutionError {
                    reducer: None,
                    reducer_id: None,
                    caller_identity: client.id.identity,
                    caller_connection_id: Some(client.id.connection_id),
                    err,
                }
                .into());
            }
        }
        ClientMessage::CallReducer(ws_v2::CallReducer {
            ref reducer,
            args,
            request_id,
            flags: _,
        }) => {
            // V2 CallReducerFlags::Default maps to v1 FullUpdate behavior.
            let flags = spacetimedb_client_api_messages::websocket::v1::CallReducerFlags::FullUpdate;
            let res = client
                .call_reducer(reducer, FunctionArgs::Bsatn(args), request_id, timer, flags)
                .await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Reducer, &database_identity, reducer)
                .observe(timer.elapsed().as_secs_f64());
            res.map(drop)
                .map_err(|e| super::message_handlers_v1::MessageExecutionError {
                    reducer: Some(reducer.clone()),
                    reducer_id: mod_info.module_def.reducer_full(&**reducer).map(|(id, _)| id),
                    caller_identity: client.id.identity,
                    caller_connection_id: Some(client.id.connection_id),
                    err: e.into(),
                })?;
        }
        ClientMessage::CallProcedure(ws_v2::CallProcedure {
            ref procedure,
            args,
            request_id,
            flags: _,
        }) => {
            let res = client
                .call_procedure(procedure, FunctionArgs::Bsatn(args), request_id, timer)
                .await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Procedure, &database_identity, procedure)
                .observe(timer.elapsed().as_secs_f64());
            if let Err(e) = res {
                log::warn!("Procedure call failed: {e:#}");
            }
            // `ClientConnection::call_procedure` handles sending the error message to the client if the call fails,
            // so we don't need to return an `Err` here.
        }
    }

    Ok(())
}
