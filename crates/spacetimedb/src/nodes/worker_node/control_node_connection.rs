

use std::{time::Duration, sync::{Mutex, Arc}};
use futures::StreamExt;
use hyper::{Uri, Request, Body, body};
use prost::Message;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;
use crate::{protobuf::{control_worker_api::{WorkerBoundMessage, worker_bound_message, schedule_update, insert_operation, update_operation, delete_operation, ScheduleUpdate, ScheduleState}, control_db::DatabaseInstance, worker_db::DatabaseInstanceState}, hash::Hash, nodes::worker_node::worker_db, db::relational_db::RelationalDB};

use super::{wasm_host_controller, database_logger::DatabaseLogger, worker_database_instance::WorkerDatabaseInstance};


pub async fn start(bootstrap_addr: String) {
    let node_id = worker_db::get_node_id().unwrap();
    let uri = if let Some(node_id) = node_id {
        format!("ws://{}/join?node_id={}", bootstrap_addr, node_id).parse::<Uri>().unwrap()
    } else {
        format!("ws://{}/join", bootstrap_addr).parse::<Uri>().unwrap()
    };
    let (mut socket, node_id) = loop {
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
            .header("Sec-WebSocket-Protocol", crate::nodes::control_node::worker_api::routes::BIN_PROTOCOL)
            .uri(&uri)
            .body(())
            .unwrap();

        match connect_async(request).await {
            Ok((socket, response)) => {
                let node_id = response.headers().get("spacetimedb-node-id").unwrap().to_str().unwrap().parse::<u64>().unwrap();
                break (socket, node_id);
            },
            Err(err) => {
                let millis = 5000;
                log::debug!("Error connecting to control node: {:?}", err);
                log::debug!("Retrying connection in {} ms", millis);
                sleep(Duration::from_millis(millis)).await;
            },
        }
    };

    worker_db::set_node_id(node_id).unwrap();

    while let Some(message) = socket.next().await {
        match message {
            Ok(WebSocketMessage::Text(_)) => {
                break;
            }
            Ok(WebSocketMessage::Binary(message_buf)) => {
                if let Err(e) = on_binary(&bootstrap_addr, node_id, message_buf).await {
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

async fn on_binary(bootstrap_addr: &str, node_id: u64, message: Vec<u8>) -> Result<(), anyhow::Error> {
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
            on_schedule_update(bootstrap_addr, node_id, schedule_update).await;
        },
        worker_bound_message::Type::ScheduleState(schedule_state) => {
            on_schedule_state(bootstrap_addr, node_id, schedule_state).await;
        },
    };
    Ok(())
}

async fn on_schedule_state(bootstrap_addr: &str, node_id: u64, schedule_state: ScheduleState) {
    println!("node_id: {}", node_id);
    println!("schedule_state: {:?}", schedule_state);
    worker_db::init_with_schedule_state(schedule_state);

    for instance in worker_db::get_database_instances() {
        on_insert_database_instance(bootstrap_addr, instance).await;
    }
}

async fn on_schedule_update(bootstrap_addr: &str, node_id: u64, schedule_update: ScheduleUpdate) {
    println!("node_id: {}", node_id);
    println!("schedule_update: {:?}", schedule_update);
    match schedule_update.r#type {
        Some(schedule_update::Type::Insert(insert_operation)) => {
            match insert_operation.r#type {
                Some(insert_operation::Type::DatabaseInstance(database_instance)) => {
                    on_insert_database_instance(bootstrap_addr, database_instance).await;
                },
                Some(insert_operation::Type::Database(database)) => {
                    worker_db::insert_database(database);
                }
                None => {
                    log::debug!("Not supposed to happen.");
                },
            }
        },
        Some(schedule_update::Type::Update(update_operation)) => {
            match update_operation.r#type {
                Some(update_operation::Type::DatabaseInstance(database_instance)) => {
                    on_update_database_instance(bootstrap_addr, database_instance).await;
                },
                Some(update_operation::Type::Database(database)) => {
                    worker_db::insert_database(database);
                },
                None => {
                    log::debug!("Not supposed to happen.");
                },
            }
        },
        Some(schedule_update::Type::Delete(delete_operation)) => {
            match delete_operation.r#type {
                Some(delete_operation::Type::DatabaseInstanceId(database_instance_id)) => {
                    on_delete_database_instance(database_instance_id).await;
                },
                Some(delete_operation::Type::DatabaseId(database_id)) => {
                    worker_db::delete_database(database_id);
                }
                None => {},
            }
        },
        None => todo!(),
    }
}

async fn on_insert_database_instance(bootstrap_addr: &str, instance: DatabaseInstance) {
    let state = worker_db::get_database_instance_state(instance.id).unwrap();
    if let Some(mut state) = state {
        if !state.initialized {
            // Start and init the service
            init_module_on_database_instance(bootstrap_addr, instance.database_id, instance.id).await;
            state.initialized = true;
            worker_db::upsert_database_instance_state(state).unwrap();
        } else {
            start_module_on_database_instance(bootstrap_addr, instance.database_id, instance.id).await;
        }
    } else {
        // Start and init the service
        let mut state = DatabaseInstanceState {
            database_instance_id: instance.id,
            initialized: false,
        };
        worker_db::upsert_database_instance_state(state.clone()).unwrap();
        init_module_on_database_instance(bootstrap_addr, instance.database_id, instance.id).await;
        state.initialized = true;
        worker_db::upsert_database_instance_state(state).unwrap();
    }
}

async fn on_update_database_instance(bootstrap_addr: &str, instance: DatabaseInstance) {
    // This logic is the same right now
    on_insert_database_instance(bootstrap_addr, instance).await;
}

async fn on_delete_database_instance(instance_id: u64) {
    let state = worker_db::get_database_instance_state(instance_id).unwrap();
    if let Some(_state) = state {
        let host = wasm_host_controller::get_host();
        let _address = host.delete_module(instance_id).await.unwrap();
        worker_db::delete_database_instance(instance_id);
    }
}

async fn get_wasm_bytes(bootstrap_addr: &str, wasm_bytes_address: &Hash) -> Vec<u8> {
    let uri = format!("http://{}/wasm_bytes/{}", bootstrap_addr, wasm_bytes_address.to_hex()).parse::<Uri>().unwrap();

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .body(Body::empty())
        .unwrap();    
   
    let client = hyper::Client::new();
    let res = client.request(request).await.unwrap();
    let body = res.into_body();
    let bytes = body::to_bytes(body).await.unwrap();

    bytes.to_vec()
}

async fn init_module_on_database_instance(bootstrap_addr: &str, database_id: u64, instance_id: u64) {
    let database = worker_db::get_database_by_id(database_id).unwrap();
    let identity = Hash::from_slice(database.identity);
    let name = database.name;
    let wasm_bytes_address = Hash::from_slice(database.wasm_bytes_address);
    let wasm_bytes = get_wasm_bytes(bootstrap_addr, &wasm_bytes_address).await;
    let root = format!("/stdb/worker_node/database_instances");
    let log_path = format!("{}/{}/{}/{}/{}", root, identity.to_hex(), name, instance_id, "module_logs");
    let db_path = format!("{}/{}/{}/{}/{}", root, identity.to_hex(), name, instance_id, "database");

    let worker_database_instance = WorkerDatabaseInstance {
        database_instance_id: instance_id,
        database_id,
        identity,
        name: name.clone(),
        logger: Arc::new(Mutex::new(DatabaseLogger::open(&log_path, &format!("{}.log", name)))),
        relational_db: Arc::new(Mutex::new(RelationalDB::open(db_path))),
    };

    let host = wasm_host_controller::get_host();
    let _address = host.init_module(worker_database_instance, wasm_bytes.clone()).await.unwrap();

    crate::logs::init_log(identity, &name);
}

async fn start_module_on_database_instance(bootstrap_addr: &str, database_id: u64, instance_id: u64) {
    let database = worker_db::get_database_by_id(database_id).unwrap();
    let identity = Hash::from_slice(database.identity);
    let name = database.name;
    let wasm_bytes_address = Hash::from_slice(database.wasm_bytes_address);
    let wasm_bytes = get_wasm_bytes(bootstrap_addr, &wasm_bytes_address).await;

    let root = format!("/stdb/worker_node/database_instances");
    let log_path = format!("{}/{}/{}/{}/{}", root, identity.to_hex(), name, instance_id, "module_logs");
    let db_path = format!("{}/{}/{}/{}/{}", root, identity.to_hex(), name, instance_id, "database");

    let worker_database_instance = WorkerDatabaseInstance {
        database_instance_id: instance_id,
        database_id,
        identity,
        name: name.clone(),
        logger: Arc::new(Mutex::new(DatabaseLogger::open(&log_path, &format!("{}.log", name)))),
        relational_db: Arc::new(Mutex::new(RelationalDB::open(db_path))),
    };

    let host = wasm_host_controller::get_host();
    let _address = host.add_module(worker_database_instance, wasm_bytes.clone()).await.unwrap();
    crate::logs::init_log(identity, &name);
}