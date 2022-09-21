use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::StreamExt;
use hyper::{body, Body, Request, StatusCode, Uri};
use int_enum::IntEnum;
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::nodes::worker_node::worker_budget;
use crate::nodes::worker_node::worker_budget::send_budget_alloc_spend;
use crate::nodes::HostType;
use crate::protobuf::control_worker_api::BudgetUpdate;
use crate::{address::Address, db::relational_db::RelationalDBWrapper};
use crate::{
    db::relational_db::RelationalDB,
    hash::Hash,
    nodes::worker_node::worker_db,
    protobuf::{
        control_db::DatabaseInstance,
        control_worker_api::{
            delete_operation, insert_operation, schedule_update, update_operation, worker_bound_message, ScheduleState,
            ScheduleUpdate, WorkerBoundMessage,
        },
        worker_db::DatabaseInstanceState,
    },
};

use super::database_instance_context_controller::DatabaseInstanceContextController;
use super::{database_logger::DatabaseLogger, host::host_controller, worker_database_instance::WorkerDatabaseInstance};

pub async fn start(worker_api_bootstrap_addr: String, client_api_bootstrap_addr: String, advertise_addr: String) {
    ControlNodeClient::set_shared(&worker_api_bootstrap_addr, &client_api_bootstrap_addr);
    let bootstrap_addr = worker_api_bootstrap_addr;

    let node_id = worker_db::get_node_id().unwrap();
    let uri = if let Some(node_id) = node_id {
        format!(
            "ws://{}/join?node_id={}&advertise_addr={}",
            bootstrap_addr,
            node_id,
            urlencoding::encode(&advertise_addr)
        )
        .parse::<Uri>()
        .unwrap()
    } else {
        format!(
            "ws://{}/join?advertise_addr={}",
            bootstrap_addr,
            urlencoding::encode(&advertise_addr)
        )
        .parse::<Uri>()
        .unwrap()
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
            .header(
                "Sec-WebSocket-Protocol",
                crate::nodes::control_node::worker_api::routes::BIN_PROTOCOL,
            )
            .uri(&uri)
            .body(())
            .unwrap();

        match connect_async(request).await {
            Ok((socket, response)) => {
                let node_id = response
                    .headers()
                    .get("spacetimedb-node-id")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                break (socket, node_id);
            }
            Err(err) => {
                let millis = 5000;
                log::debug!("Error connecting to control node: {:?}", err);
                log::debug!("Retrying connection in {} ms", millis);
                sleep(Duration::from_millis(millis)).await;
            }
        }
    };

    worker_db::set_node_id(node_id).unwrap();

    while let Some(message) = socket.next().await {
        match message {
            Ok(WebSocketMessage::Text(_)) => {
                break;
            }
            Ok(WebSocketMessage::Binary(message_buf)) => {
                if let Err(e) = on_binary(node_id, message_buf, &mut socket).await {
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

async fn on_binary(
    node_id: u64,
    message: Vec<u8>,
    socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> Result<(), anyhow::Error> {
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
            on_schedule_update(node_id, schedule_update).await;
        }
        worker_bound_message::Type::ScheduleState(schedule_state) => {
            on_schedule_state(node_id, schedule_state).await;
        }
        worker_bound_message::Type::BudgetUpdate(budget_update) => {
            // Budget update logic.
            // Control node is sending us an allocation based on the last spend information we
            // sent them.

            // First we will let them know what we spent, which will be taken into account for the
            // *next* budget update they send us.
            send_budget_alloc_spend(socket).await.unwrap();

            // Then adjust our allocation based on what they just sent us, which will also reset
            // our "spent" value.
            on_worker_budget_update(budget_update);
        }
    };
    Ok(())
}

fn on_worker_budget_update(budget_update: BudgetUpdate) {
    worker_budget::on_budget_receive_allocation(
        &Hash::from_slice(budget_update.module_identity),
        budget_update.allocation_delta,
        budget_update.default_max_spend,
    );
}

async fn on_schedule_state(_node_id: u64, schedule_state: ScheduleState) {
    worker_db::init_with_schedule_state(schedule_state);

    for instance in worker_db::get_database_instances() {
        on_insert_database_instance(instance).await;
    }
}

async fn on_schedule_update(_node_id: u64, schedule_update: ScheduleUpdate) {
    match schedule_update.r#type {
        Some(schedule_update::Type::Insert(insert_operation)) => match insert_operation.r#type {
            Some(insert_operation::Type::DatabaseInstance(database_instance)) => {
                worker_db::insert_database_instance(database_instance.clone());
                on_insert_database_instance(database_instance).await;
            }
            Some(insert_operation::Type::Database(database)) => {
                worker_db::insert_database(database);
            }
            None => {
                log::debug!("Not supposed to happen.");
            }
        },
        Some(schedule_update::Type::Update(update_operation)) => match update_operation.r#type {
            Some(update_operation::Type::DatabaseInstance(database_instance)) => {
                worker_db::insert_database_instance(database_instance.clone());
                on_update_database_instance(database_instance).await;
            }
            Some(update_operation::Type::Database(database)) => {
                worker_db::insert_database(database);
            }
            None => {
                log::debug!("Not supposed to happen.");
            }
        },
        Some(schedule_update::Type::Delete(delete_operation)) => match delete_operation.r#type {
            Some(delete_operation::Type::DatabaseInstanceId(database_instance_id)) => {
                worker_db::delete_database_instance(database_instance_id);
                on_delete_database_instance(database_instance_id).await;
            }
            Some(delete_operation::Type::DatabaseId(database_id)) => {
                worker_db::delete_database(database_id);
            }
            None => {}
        },
        None => todo!(),
    }
}

async fn on_insert_database_instance(instance: DatabaseInstance) {
    let state = worker_db::get_database_instance_state(instance.id).unwrap();
    if let Some(mut state) = state {
        if !state.initialized {
            // Start and init the service
            init_module_on_database_instance(instance.database_id, instance.id).await;
            state.initialized = true;
            worker_db::upsert_database_instance_state(state).unwrap();
        } else {
            start_module_on_database_instance(instance.database_id, instance.id).await;
        }
    } else {
        // Start and init the service
        let mut state = DatabaseInstanceState {
            database_instance_id: instance.id,
            initialized: false,
        };
        worker_db::upsert_database_instance_state(state.clone()).unwrap();
        init_module_on_database_instance(instance.database_id, instance.id).await;
        state.initialized = true;
        worker_db::upsert_database_instance_state(state).unwrap();
    }
}

async fn on_update_database_instance(instance: DatabaseInstance) {
    // This logic is the same right now
    on_insert_database_instance(instance).await;
}

async fn on_delete_database_instance(instance_id: u64) {
    let state = worker_db::get_database_instance_state(instance_id).unwrap();
    if let Some(_state) = state {
        let host = host_controller::get_host();

        // TODO: This is getting pretty messy
        DatabaseInstanceContextController::get_shared().remove(instance_id);
        let _address = host.delete_module(instance_id).await.unwrap();
        worker_db::delete_database_instance(instance_id);
    }
}

async fn init_module_on_database_instance(database_id: u64, instance_id: u64) {
    let database = if let Some(database) = worker_db::get_database_by_id(database_id) {
        database
    } else {
        return;
    };
    let identity = Hash::from_slice(database.identity);
    let address = Address::from_slice(database.address);
    let program_bytes_address = Hash::from_slice(database.program_bytes_address);
    let program_bytes = ControlNodeClient::get_shared()
        .get_program_bytes(&program_bytes_address)
        .await;

    let log_path = DatabaseLogger::filepath(&address, instance_id);
    let root = format!("/stdb/worker_node/database_instances");
    let db_path = format!("{}/{}/{}/{}", root, address.to_hex(), instance_id, "database");

    let worker_database_instance = WorkerDatabaseInstance {
        database_instance_id: instance_id,
        database_id,
        host_type: HostType::from_int(database.host_type).expect("unknown module host type"),
        identity,
        address,
        logger: Arc::new(Mutex::new(DatabaseLogger::open(&log_path))),
        relational_db: RelationalDBWrapper::new(RelationalDB::open(db_path)),
    };

    // TODO: This is getting pretty messy
    DatabaseInstanceContextController::get_shared().insert(worker_database_instance.clone());
    let host = host_controller::get_host();
    let _address = host
        .init_module(worker_database_instance, program_bytes.clone())
        .await
        .unwrap();
}

async fn start_module_on_database_instance(database_id: u64, instance_id: u64) {
    let database = if let Some(database) = worker_db::get_database_by_id(database_id) {
        database
    } else {
        return;
    };
    let identity = Hash::from_slice(database.identity);
    let address = Address::from_slice(database.address);
    let program_bytes_address = Hash::from_slice(database.program_bytes_address);
    let program_bytes = ControlNodeClient::get_shared()
        .get_program_bytes(&program_bytes_address)
        .await;

    let log_path = DatabaseLogger::filepath(&address, instance_id);
    let root = format!("/stdb/worker_node/database_instances");
    let db_path = format!("{}/{}/{}/{}", root, address.to_hex(), instance_id, "database");

    let worker_database_instance = WorkerDatabaseInstance {
        database_instance_id: instance_id,
        database_id,
        host_type: HostType::from_int(database.host_type).expect("unknown module host type"),
        identity,
        address,
        logger: Arc::new(Mutex::new(DatabaseLogger::open(&log_path))),
        relational_db: RelationalDBWrapper::new(RelationalDB::open(db_path)),
    };

    // TODO: This is getting pretty messy
    DatabaseInstanceContextController::get_shared().insert(worker_database_instance.clone());
    let host = host_controller::get_host();
    let _address = host
        .add_module(worker_database_instance, program_bytes.clone())
        .await
        .unwrap();
}

lazy_static::lazy_static! {
    static ref CONTROL_NODE_CLIENT: Mutex<Option<ControlNodeClient>> = Mutex::new(None);
}

#[derive(Debug, Clone)]
pub struct ControlNodeClient {
    pub worker_api_bootstrap_addr: String,
    pub client_api_bootstrap_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DNSResponse {
    address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityResponse {
    identity: String,
    token: String,
}

impl ControlNodeClient {
    fn set_shared(worker_api_bootstrap_addr: &str, client_api_bootstrap_addr: &str) {
        *CONTROL_NODE_CLIENT.lock().unwrap() = Some(ControlNodeClient {
            worker_api_bootstrap_addr: worker_api_bootstrap_addr.to_string(),
            client_api_bootstrap_addr: client_api_bootstrap_addr.to_string(),
        })
    }

    pub fn get_shared() -> Self {
        CONTROL_NODE_CLIENT.lock().unwrap().clone().unwrap()
    }

    pub async fn get_new_identity(&self) -> Result<(Hash, String), anyhow::Error> {
        let uri = format!("http://{}/identity", self.client_api_bootstrap_addr)
            .parse::<Uri>()
            .unwrap();

        let request = Request::builder().method("POST").uri(&uri).body(Body::empty()).unwrap();

        let client = hyper::Client::new();
        let res = client.request(request).await.unwrap();
        let body = res.into_body();
        let bytes = body::to_bytes(body).await.unwrap();
        let res: IdentityResponse = serde_json::from_slice(&bytes[..])?;

        Ok((Hash::from_hex(&res.identity).unwrap(), res.token))
    }

    pub async fn resolve_name(&self, name: &str) -> Result<Option<Address>, anyhow::Error> {
        let uri = format!("http://{}/database/dns/{}", self.client_api_bootstrap_addr, name)
            .parse::<Uri>()
            .unwrap();

        let request = Request::builder().method("POST").uri(&uri).body(Body::empty())?;

        let client = hyper::Client::new();
        let res = client.request(request).await.unwrap();

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let body = res.into_body();
        let bytes = body::to_bytes(body).await.unwrap();
        let res: DNSResponse = serde_json::from_slice(&bytes[..])?;

        Ok(Some(Address::from_hex(&res.address).unwrap()))
    }

    async fn get_program_bytes(&self, program_bytes_address: &Hash) -> Vec<u8> {
        let uri = format!(
            "http://{}/program_bytes/{}",
            self.worker_api_bootstrap_addr,
            program_bytes_address.to_hex()
        )
        .parse::<Uri>()
        .unwrap();

        let request = Request::builder().method("GET").uri(&uri).body(Body::empty()).unwrap();

        let client = hyper::Client::new();
        let res = client.request(request).await.unwrap();
        let body = res.into_body();
        let bytes = body::to_bytes(body).await.unwrap();

        bytes.to_vec()
    }

    pub async fn _init_database(&self, address: &Address, program_bytes: Vec<u8>, host_type: HostType, force: bool) {
        let force_str = if force { "true" } else { "false" };
        let uri = format!(
            "http://{}/database/init/{}?force={}&host_type={}",
            self.client_api_bootstrap_addr,
            address.to_hex(),
            force_str,
            host_type.as_param_str()
        )
        .parse::<Uri>()
        .unwrap();

        let request = Request::builder()
            .method("POST")
            .uri(&uri)
            .body(Body::from(program_bytes))
            .unwrap();

        let client = hyper::Client::new();
        let res = client.request(request).await.unwrap();
        if !res.status().is_success() {
            todo!("handle this: {:?}", res);
        }
    }

    pub async fn _update_database(&self, address: &Address, program_bytes: Vec<u8>) {
        let uri = format!(
            "http://{}/database/update/{}",
            self.client_api_bootstrap_addr,
            address.to_hex()
        )
        .parse::<Uri>()
        .unwrap();

        let request = Request::builder()
            .method("POST")
            .uri(&uri)
            .body(Body::from(program_bytes))
            .unwrap();

        let client = hyper::Client::new();
        let res = client.request(request).await.unwrap();
        if !res.status().is_success() {
            todo!("handle this: {:?}", res);
        }
    }

    pub async fn _delete_database(&self, address: &Address) {
        let uri = format!(
            "http://{}/database/delete/{}",
            self.client_api_bootstrap_addr,
            address.to_hex()
        )
        .parse::<Uri>()
        .unwrap();

        let request = Request::builder().method("POST").uri(&uri).body(Body::empty()).unwrap();

        let client = hyper::Client::new();
        let res = client.request(request).await.unwrap();
        if !res.status().is_success() {
            todo!("handle this: {:?}", res);
        }
    }
}
