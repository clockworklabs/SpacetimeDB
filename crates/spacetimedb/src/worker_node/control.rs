use std::{time::Duration, sync::Mutex, collections::HashMap};
use futures::StreamExt;
use hyper::{Uri, Request};
use lazy_static::lazy_static;
use prost::Message;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;
use crate::{protobuf::control_worker_api::{WorkerBoundMessage, worker_bound_message, schedule_update, insert_operation, update_operation, delete_operation, Database, DatabaseInstance}, hash::Hash};

use super::client_api;

lazy_static! {
    static ref DATABASES: Mutex<HashMap<u32, Database>> = Mutex::new(HashMap::new());
    static ref DATABASE_INSTANCES: Mutex<HashMap<u32, DatabaseInstance>> = Mutex::new(HashMap::new());
}

pub async fn start(bootstrap_addr: String) {
    let uri = format!("ws://{}/join", bootstrap_addr).parse::<Uri>().unwrap();
    let mut socket = loop {
        let authority = uri.authority().unwrap().as_str();
        let host = authority
            .find('@')
            .map(|idx| authority.split_at(idx + 1).1)
            .unwrap_or_else(|| authority);

        let request = Request::builder()
            .method("GET")
            .header("Host", host)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", generate_key())
            .header("Sec-WebSocket-Protocol", crate::control_node::worker_api::routes::BIN_PROTOCOL)
            .uri(&uri)
            .body(())
            .unwrap();

        match connect_async(request).await {
            Ok(result) => {
                let (socket, _) = result;
                break socket;
            },
            Err(err) => {
                let millis = 5000;
                log::debug!("Error connecting to control node: {:?}", err);
                log::debug!("Retrying connection in {} ms", millis);
                sleep(Duration::from_millis(millis)).await;
            },
        }
    };

    while let Some(message) = socket.next().await {
        match message {
            Ok(WebSocketMessage::Text(_)) => {
                break;
            }
            Ok(WebSocketMessage::Binary(message_buf)) => {
                if let Err(e) = on_binary(message_buf).await {
                    log::debug!("Worker caused error on binary message: {}", e);
                    break;
                }
            }
            Ok(WebSocketMessage::Ping(_message)) => {
                log::trace!("Received ping from control node.");
            }
            Ok(WebSocketMessage::Pong(_message)) => {
                log::trace!("Received pong from control node.");
            }
            Ok(WebSocketMessage::Close(close_frame)) => {
                // This can mean 1 of 2 things:
                //
                // 1. The client has sent an unsolicited close frame.
                // This means the client wishes to close the connection
                // and will send no further messages along the
                // connection. Don't destroy the connection yet.
                // Wait for the stream to end.
                // NOTE: No need to send a close message, this is sent
                // automatically by tungstenite.
                //
                // 2. We sent a close frame and the library is telling us about
                // it. Very silly if you ask me.
                // There's no need to do anything here, because we're the ones
                // that sent the initial close. The close frame we just received
                // was an acknowledgement by the client (their side of the handshake)
                // Maybe check their close frame or something
                log::trace!("Close frame {:?}", close_frame);
            }
            Ok(WebSocketMessage::Frame(_)) => {
                // TODO: I don't know what this is for, since it's new
                // I assume probably for sending large files?
            }
            Err(error) => match error {
                tokio_tungstenite::tungstenite::Error::ConnectionClosed => {
                    // Do nothing. There's no need to listen to this error because
                    // according to the tungstenite documentation its really more of a
                    // notification anyway, and tokio-tungstenite will end the stream
                    // so we'll drop the websocket at after the while loop.
                }
                error => log::warn!("Websocket receive error: {}", error),
            },
        }
    }
}

async fn on_binary(message: Vec<u8>) -> Result<(), anyhow::Error> {
    let message = WorkerBoundMessage::decode(&message[..]);
    let message = match message {
        Ok(message) => message,
        Err(error) => {
            log::warn!("Control node sent poorly formed message: {}", error);
            return Err(anyhow::anyhow!("{:?}", error));
        }
    };
    let message = match message.r#type {
        Some(value) => value,
        None => {
            log::warn!("Control node sent a message with no type");
            return Err(anyhow::anyhow!("Control node sent a message with no type"));
        }
    };
    match message {
        worker_bound_message::Type::ScheduleUpdate(schedule_update) => {
            match schedule_update.r#type {
                Some(schedule_update::Type::Insert(insert_operation)) => {
                    match insert_operation.r#type {
                        Some(insert_operation::Type::DatabaseInstance(database_instance)) => {
                            let (identity, name, wasm_bytes) = {
                                let database_id = database_instance.database_id;
                                let mut database_instances = DATABASE_INSTANCES.lock().unwrap();
                                database_instances.insert(database_instance.id, database_instance);

                                let databases = DATABASES.lock().unwrap();
                                let database = databases.get(&database_id).unwrap();

                                let identity = Hash::from_iter(database.identity.clone());
                                let wasm_bytes = database.wasm_bytes_address.clone();
                                // TODO!
                                (identity, database.name.clone(), wasm_bytes)
                            };
                            client_api::api::init_module(&identity, &name, wasm_bytes).await?;
                        },
                        None => {},
                    }
                },
                Some(schedule_update::Type::Update(update_operation)) => {
                    match update_operation.r#type {
                        Some(update_operation::Type::DatabaseInstance(database_instance)) => {
                            let (identity, name, wasm_bytes) = {
                                let database_id = database_instance.database_id;
                                let mut database_instances = DATABASE_INSTANCES.lock().unwrap();
                                database_instances.insert(database_instance.id, database_instance);

                                let databases = DATABASES.lock().unwrap();
                                let database = databases.get(&database_id).unwrap();

                                let identity = Hash::from_iter(database.identity.clone());
                                let wasm_bytes = database.wasm_bytes_address.clone();
                                // TODO!
                                (identity, database.name.clone(), wasm_bytes)
                            };
                            client_api::api::update_module(&identity, &name, wasm_bytes).await?;
                        },
                        None => {},
                    }
                },
                Some(schedule_update::Type::Delete(delete_operation)) => {
                    match delete_operation.r#type {
                        Some(delete_operation::Type::DatabaseInstanceId(database_instance_id)) => {
                            let (identity, name, wasm_bytes) = {
                                let mut database_instances = DATABASE_INSTANCES.lock().unwrap();
                                let database_instance = database_instances.remove(&database_instance_id).unwrap();

                                let database_id = database_instance.database_id;

                                let databases = DATABASES.lock().unwrap();
                                let database = databases.get(&database_id).unwrap();

                                let identity = Hash::from_iter(database.identity.clone());
                                let wasm_bytes = database.wasm_bytes_address.clone(); // TODO!
                                (identity, database.name.clone(), wasm_bytes)
                            };
                            client_api::api::delete_module(&identity, &name).await?;
                        },
                        None => {},
                    }
                },
                None => todo!(),
            }
        },
        worker_bound_message::Type::ScheduleState(schedule_state) => {
            for instance in schedule_state.database_instances {
                let mut database_instances = DATABASE_INSTANCES.lock().unwrap();
                database_instances.insert(instance.id, instance);
            }
            let database_instances = DATABASE_INSTANCES.lock().unwrap();
            for database_instance in database_instances.values() {
                let (identity, name, wasm_bytes) = {
                    let database_id = database_instance.database_id;
                    let mut database_instances = DATABASE_INSTANCES.lock().unwrap();
                    database_instances.insert(database_instance.id, database_instance);

                    let databases = DATABASES.lock().unwrap();
                    let database = databases.get(&database_id).unwrap();

                    let identity = Hash::from_iter(database.identity.clone());
                    let wasm_bytes = database.wasm_bytes_address.clone();
                    // TODO!
                    (identity, database.name.clone(), wasm_bytes)
                };
                client_api::api::start_module(&identity, &name, wasm_bytes).await?;
            } 
        },
    };
    Ok(())
}