use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, FromRef, Path, Query, State};
use axum::response::{ErrorResponse, IntoResponse};
use axum::{headers, TypedHeader};
use futures::StreamExt;
use http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use spacetimedb::host::EntityDef;
use spacetimedb::host::ReducerArgs;
use spacetimedb::host::ReducerCallError;
use spacetimedb::host::ReducerOutcome;
use spacetimedb::host::UpdateDatabaseSuccess;
use spacetimedb_lib::name;
use spacetimedb_lib::name::DomainName;
use spacetimedb_lib::name::DomainParsingError;
use spacetimedb_lib::name::PublishOp;
use spacetimedb_lib::sats::TypeInSpace;

use crate::auth::{
    SpacetimeAuth, SpacetimeAuthHeader, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros,
    SpacetimeIdentity, SpacetimeIdentityToken,
};
use spacetimedb::address::Address;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::DescribedEntityType;
use spacetimedb::identity::Identity;
use spacetimedb::json::client_api::StmtResultJson;
use spacetimedb::messages::control_db::{Database, DatabaseInstance, HostType};

use crate::util::{ByteStringBody, NameOrAddress};
use crate::{log_and_500, ControlCtx, ControlNodeDelegate, WorkerCtx};

pub(crate) struct DomainParsingRejection(pub(crate) DomainParsingError);
impl From<DomainParsingError> for DomainParsingRejection {
    fn from(e: DomainParsingError) -> Self {
        DomainParsingRejection(e)
    }
}
impl IntoResponse for DomainParsingRejection {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, "Unable to parse domain name").into_response()
    }
}

#[derive(Deserialize)]
pub struct CallParams {
    name_or_address: NameOrAddress,
    reducer: String,
}

pub async fn call(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    auth: SpacetimeAuthHeader,
    Path(CallParams {
        name_or_address,
        reducer,
    }): Path<CallParams>,
    ByteStringBody(body): ByteStringBody,
) -> axum::response::Result<impl IntoResponse> {
    let SpacetimeAuth {
        identity: caller_identity,
        creds: caller_identity_token,
    } = auth.get_or_create(&*worker_ctx).await?;

    let args = ReducerArgs::Json(body);

    let address = name_or_address.resolve(&*worker_ctx).await?;
    let database = worker_ctx_find_database(&*worker_ctx, &address)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", address.to_hex());
            (StatusCode::NOT_FOUND, "No such database.")
        })?;
    let identity = database.identity;
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
        .await
        .ok_or((
            StatusCode::NOT_FOUND,
            "Database instance not scheduled to this node yet.",
        ))?;
    let instance_id = database_instance.id;
    let host = worker_ctx.host_controller();

    let module = match host.get_module_host(instance_id) {
        Ok(m) => m,
        Err(_) => {
            let dbic = worker_ctx
                .load_module_host_context(database, instance_id)
                .await
                .map_err(log_and_500)?;
            host.spawn_module_host(dbic).await.map_err(log_and_500)?
        }
    };
    let result = match module
        .call_reducer(caller_identity, None, &reducer, args)
        .await
    {
        Ok(rcr) => rcr,
        Err(e) => {
            let status_code = match e {
                ReducerCallError::Args(_) => {
                    log::debug!("Attempt to call reducer with invalid arguments");
                    StatusCode::BAD_REQUEST
                }
                ReducerCallError::NoSuchModule(_) => StatusCode::NOT_FOUND,
                ReducerCallError::NoSuchReducer => {
                    log::debug!("Attempt to call non-existent reducer {}", reducer);
                    StatusCode::NOT_FOUND
                }
            };

            log::debug!("Error while invoking reducer {:#}", e);
            return Err((status_code, format!("{:#}", anyhow::anyhow!(e))).into());
        }
    };

    let (status, body) = reducer_outcome_response(&identity, &reducer, result.outcome);
    Ok((
        status,
        TypedHeader(SpacetimeIdentity(caller_identity)),
        TypedHeader(SpacetimeIdentityToken(caller_identity_token)),
        TypedHeader(SpacetimeEnergyUsed(result.energy_used)),
        TypedHeader(SpacetimeExecutionDurationMicros(result.execution_duration)),
        body,
    ))
}

fn reducer_outcome_response(
    identity: &Identity,
    reducer: &str,
    outcome: ReducerOutcome,
) -> (StatusCode, String) {
    match outcome {
        ReducerOutcome::Committed => (StatusCode::OK, "".to_owned()),
        ReducerOutcome::Failed(errmsg) => {
            // TODO: different status code? this is what cloudflare uses, sorta
            (StatusCode::from_u16(530).unwrap(), errmsg)
        }
        ReducerOutcome::BudgetExceeded => {
            log::warn!(
                "Node's energy budget exceeded for identity: {} while executing {}",
                identity,
                reducer
            );
            (
                StatusCode::PAYMENT_REQUIRED,
                "Module energy budget exhausted.".to_owned(),
            )
        }
    }
}

#[derive(Debug)]
pub enum DBCallErr {
    HandlerError(ErrorResponse),
    NoSuchDatabase,
    InstanceNotScheduled,
}

use chrono::Utc;
use rand::Rng;
use spacetimedb::auth::identity::encode_token;
use spacetimedb::sql::execute::execute;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::name::{DnsLookupResponse, InsertDomainResult, PublishResult};
use spacetimedb_lib::recovery::{RecoveryCode, RecoveryCodeResponse};
use std::convert::From;

impl From<ErrorResponse> for DBCallErr {
    fn from(error: ErrorResponse) -> Self {
        DBCallErr::HandlerError(error)
    }
}

pub struct DatabaseInformation {
    database_instance: DatabaseInstance,
    auth: SpacetimeAuth,
}
/// Extract some common parameters that most API call invocations to the database will use.
/// TODO(tyler): Ryan originally intended for extract call info to be used for any call that is specific to a
/// database. However, there are some functions that should be callable from anyone, possibly even if they
/// don't provide any credentials at all. The problem is that this function doesn't make sense in all places
/// where credentials are required (e.g. publish), so for now we're just going to keep this as is, but we're
/// going to generate a new set of credentials if you don't provide them.
async fn extract_db_call_info(
    ctx: &dyn WorkerCtx,
    auth: SpacetimeAuthHeader,
    address: &Address,
) -> Result<DatabaseInformation, ErrorResponse> {
    let auth = auth.get_or_create(ctx).await?;

    let database = worker_ctx_find_database(ctx, address)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", address.to_hex());
            (StatusCode::NOT_FOUND, "No such database.")
        })?;

    let database_instance = ctx
        .get_leader_database_instance_by_database(database.id)
        .await
        .ok_or((
            StatusCode::NOT_FOUND,
            "Database instance not scheduled to this node yet.",
        ))?;

    Ok(DatabaseInformation {
        database_instance,
        auth,
    })
}

fn entity_description_json(description: TypeInSpace<EntityDef>, expand: bool) -> Option<Value> {
    let typ = DescribedEntityType::from_entitydef(description.ty()).as_str();
    let len = match description.ty() {
        EntityDef::Table(t) => description
            .resolve(t.data)
            .ty()
            .as_product()?
            .elements
            .len(),
        EntityDef::Reducer(r) => r.args.len(),
    };
    if expand {
        // TODO(noa): make this less hacky; needs coordination w/ spacetime-web
        let schema = match description.ty() {
            EntityDef::Table(table) => {
                json!(description.with(&table.data).resolve_refs()?.as_product()?)
            }
            EntityDef::Reducer(r) => json!({
                "name": r.name,
                "elements": r.args,
            }),
        };
        Some(json!({
            "type": typ,
            "arity": len,
            "schema": schema
        }))
    } else {
        Some(json!({
            "type": typ,
            "arity": len,
        }))
    }
}

#[derive(Deserialize)]
pub struct DescribeParams {
    name_or_address: NameOrAddress,
    entity_type: String,
    entity: String,
}

#[derive(Deserialize)]
pub struct DescribeQueryParams {
    expand: Option<bool>,
}

pub async fn describe(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    Path(DescribeParams {
        name_or_address,
        entity_type,
        entity,
    }): Path<DescribeParams>,
    Query(DescribeQueryParams { expand }): Query<DescribeQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let address = name_or_address.resolve(&*worker_ctx).await?;
    let database = worker_ctx_find_database(&*worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let call_info = extract_db_call_info(&*worker_ctx, auth, &address).await?;

    let instance_id = call_info.database_instance.id;
    let host = worker_ctx.host_controller();
    let module = match host.get_module_host(instance_id) {
        Ok(m) => m,
        Err(_) => {
            let dbic = worker_ctx
                .load_module_host_context(database, instance_id)
                .await
                .map_err(log_and_500)?;
            host.spawn_module_host(dbic).await.map_err(log_and_500)?
        }
    };

    let entity_type = entity_type.as_str().parse().map_err(|()| {
        log::debug!("Request to describe unhandled entity type: {}", entity_type);
        (
            StatusCode::NOT_FOUND,
            format!("Invalid entity type for description: {}", entity_type),
        )
    })?;
    let catalog = module.catalog();
    let description = catalog
        .get(&entity)
        .filter(|desc| DescribedEntityType::from_entitydef(desc.ty()) == entity_type)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("{entity_type} {entity:?} not found"),
            )
        })?;

    let expand = expand.unwrap_or(true);
    let response_json = json!({ entity: entity_description_json(description, expand) });

    Ok((
        StatusCode::OK,
        TypedHeader(SpacetimeIdentity(call_info.auth.identity)),
        TypedHeader(SpacetimeIdentityToken(call_info.auth.creds)),
        axum::Json(response_json),
    ))
}

#[derive(Deserialize)]
pub struct CatalogParams {
    name_or_address: NameOrAddress,
}
pub async fn catalog(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    Path(CatalogParams { name_or_address }): Path<CatalogParams>,
    Query(DescribeQueryParams { expand }): Query<DescribeQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let address = name_or_address.resolve(&*worker_ctx).await?;
    let database = worker_ctx_find_database(&*worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let call_info = extract_db_call_info(&*worker_ctx, auth, &address).await?;

    let instance_id = call_info.database_instance.id;
    let host = worker_ctx.host_controller();
    let module = match host.get_module_host(instance_id) {
        Ok(m) => m,
        Err(_) => {
            let dbic = worker_ctx
                .load_module_host_context(database, instance_id)
                .await
                .map_err(log_and_500)?;
            host.spawn_module_host(dbic).await.map_err(log_and_500)?
        }
    };
    let catalog = module.catalog();
    let expand = expand.unwrap_or(false);
    let response_catalog: HashMap<_, _> = catalog
        .iter()
        .map(|(name, entity)| (name, entity_description_json(entity, expand)))
        .collect();
    let response_json = json!({
        "entities": response_catalog,
        "typespace": catalog.typespace().types,
    });

    Ok((
        StatusCode::OK,
        TypedHeader(SpacetimeIdentity(call_info.auth.identity)),
        TypedHeader(SpacetimeIdentityToken(call_info.auth.creds)),
        axum::Json(response_json),
    ))
}

#[derive(Deserialize)]
pub struct InfoParams {
    name_or_address: NameOrAddress,
}
pub async fn info(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    Path(InfoParams { name_or_address }): Path<InfoParams>,
) -> axum::response::Result<impl IntoResponse> {
    let address = name_or_address.resolve(&*worker_ctx).await?;
    let database = worker_ctx_find_database(&*worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let host_type = match database.host_type {
        HostType::Wasmer => "wasmer",
    };
    let response_json = json!({
        "address": database.address.to_hex(),
        "identity": database.identity,
        "host_type": host_type,
        "num_replicas": database.num_replicas,
        "program_bytes_address": database.program_bytes_address,
    });
    Ok((StatusCode::OK, axum::Json(response_json)))
}

#[derive(Deserialize)]
pub struct LogsParams {
    name_or_address: NameOrAddress,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    num_lines: Option<u32>,
    #[serde(default)]
    follow: bool,
}

fn auth_or_unauth(auth: SpacetimeAuthHeader) -> axum::response::Result<SpacetimeAuth> {
    auth.get()
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials").into())
}

pub async fn logs(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    Path(LogsParams { name_or_address }): Path<LogsParams>,
    Query(LogsQuery { num_lines, follow }): Query<LogsQuery>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    // You should not be able to read the logs from a database that you do not own
    // so, unless you are the owner, this will fail.
    // TODO: This returns `UNAUTHORIZED` on failure,
    //       while everywhere else we return `BAD_REQUEST`.
    //       Is this special in some way? Should this change?
    //       Should all the others change?
    let auth = auth_or_unauth(auth)?;

    let address = name_or_address.resolve(&*worker_ctx).await?;
    let database = worker_ctx_find_database(&*worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    if database.identity != auth.identity {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Identity does not own database, expected: {} got: {}",
                database.identity.to_hex(),
                auth.identity.to_hex()
            ),
        )
            .into());
    }

    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
        .await
        .ok_or((
            StatusCode::NOT_FOUND,
            "Database instance not scheduled to this node yet.",
        ))?;
    let instance_id = database_instance.id;

    let filepath = DatabaseLogger::filepath(&address, instance_id);
    let lines = DatabaseLogger::read_latest(&filepath, num_lines).await;

    let body = if follow {
        let host = worker_ctx.host_controller();
        let module = match host.get_module_host(instance_id) {
            Ok(m) => m,
            Err(_) => {
                let dbic = worker_ctx
                    .load_module_host_context(database, instance_id)
                    .await
                    .map_err(log_and_500)?;
                host.spawn_module_host(dbic).await.map_err(log_and_500)?
            }
        };
        let log_rx = module.subscribe_to_logs().map_err(log_and_500)?;

        let stream = tokio_stream::wrappers::BroadcastStream::new(log_rx).filter_map(move |x| {
            std::future::ready(match x {
                Ok(log) => Some(log),
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped)) => {
                    log::trace!(
                        "Skipped {} lines in log for module {}",
                        skipped,
                        address.to_hex()
                    );
                    None
                }
            })
        });

        let stream = futures::stream::once(std::future::ready(lines.into()))
            .chain(stream)
            .map(Ok::<_, std::convert::Infallible>);

        axum::body::boxed(axum::body::StreamBody::new(stream))
    } else {
        axum::body::boxed(axum::body::Full::from(lines))
    };

    Ok((
        StatusCode::OK,
        TypedHeader(headers::CacheControl::new().with_no_cache()),
        TypedHeader(headers::ContentType::from(mime_ndjson())),
        body,
    ))
}

fn mime_ndjson() -> mime::Mime {
    "application/x-ndjson".parse().unwrap()
}

async fn worker_ctx_find_database(
    worker_ctx: &dyn WorkerCtx,
    address: &Address,
) -> Result<Option<Database>, StatusCode> {
    worker_ctx
        .get_database_by_address(address)
        .await
        .map_err(log_and_500)
}

#[derive(Deserialize)]
pub struct SqlParams {
    name_or_address: NameOrAddress,
}

#[derive(Deserialize)]
pub struct SqlQueryParams {}

pub async fn sql(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    Path(SqlParams { name_or_address }): Path<SqlParams>,
    Query(SqlQueryParams {}): Query<SqlQueryParams>,
    auth: SpacetimeAuthHeader,
    body: String,
) -> axum::response::Result<impl IntoResponse> {
    // Anyone is authorized to execute SQL queries. The SQL engine will determine
    // which queries this identity is allowed to execute against the database.
    let auth = auth.get_or_create(&*worker_ctx).await?;

    let address = name_or_address.resolve(&*worker_ctx).await?;
    let database = worker_ctx_find_database(&*worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let auth = AuthCtx::new(database.identity, auth.identity);
    log::debug!("auth: {auth:?}");
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
        .await
        .ok_or((
            StatusCode::NOT_FOUND,
            "Database instance not scheduled to this node yet.",
        ))?;
    let instance_id = database_instance.id;

    let host = worker_ctx.host_controller();
    match host.get_module_host(instance_id) {
        Ok(_) => {}
        Err(_) => {
            let dbic = worker_ctx
                .load_module_host_context(database, instance_id)
                .await
                .map_err(log_and_500)?;
            host.spawn_module_host(dbic).await.map_err(log_and_500)?;
        }
    };

    let results = match execute(
        worker_ctx.database_instance_context_controller(),
        instance_id,
        body,
        auth,
    ) {
        Ok(results) => results,
        Err(err) => {
            log::warn!("{}", err);
            return if let Some(auth_err) = err.get_auth_error() {
                let err = format!("{auth_err}");
                Err((StatusCode::UNAUTHORIZED, err).into())
            } else {
                let err = format!("{err}");
                Err((StatusCode::BAD_REQUEST, err).into())
            };
        }
    };

    let json = results
        .into_iter()
        .map(|result| StmtResultJson {
            schema: result.head.ty(),
            rows: result
                .data
                .into_iter()
                .map(|x| x.elements)
                .collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>();

    Ok((StatusCode::OK, axum::Json(json)))
}

#[derive(Deserialize)]
pub struct DNSParams {
    database_name: String,
}

#[derive(Deserialize)]
pub struct ReverseDNSParams {
    database_address: Address,
}

#[derive(Deserialize)]
pub struct DNSQueryParams {}

pub async fn dns(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(DNSParams { database_name }): Path<DNSParams>,
    Query(DNSQueryParams {}): Query<DNSQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let domain = database_name.parse().map_err(DomainParsingRejection)?;
    let address = ctx
        .control_db()
        .spacetime_dns(&domain)
        .await
        .map_err(log_and_500)?;
    let response = if let Some(address) = address {
        DnsLookupResponse::Success {
            domain,
            address: address.to_hex(),
        }
    } else {
        DnsLookupResponse::Failure { domain }
    };

    Ok(axum::Json(response))
}

pub async fn reverse_dns(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(ReverseDNSParams { database_address }): Path<ReverseDNSParams>,
) -> axum::response::Result<impl IntoResponse> {
    let names = ctx
        .control_db()
        .spacetime_reverse_dns(&database_address)
        .await
        .map_err(log_and_500)?;

    let response = name::ReverseDNSResponse { names };
    Ok(axum::Json(response))
}

#[derive(Deserialize)]
pub struct RegisterTldParams {
    tld: String,
}

fn auth_or_bad_request(auth: SpacetimeAuthHeader) -> axum::response::Result<SpacetimeAuth> {
    auth.get()
        .ok_or((StatusCode::BAD_REQUEST, "Invalid credentials.").into())
}

pub async fn register_tld(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Query(RegisterTldParams { tld }): Query<RegisterTldParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail, hence not using get_or_create
    let auth = auth_or_bad_request(auth)?;

    let tld = tld
        .parse::<DomainName>()
        .map_err(DomainParsingRejection)?
        .into_tld();
    let result = ctx
        .control_db()
        .spacetime_register_tld(tld, auth.identity)
        .await
        .map_err(log_and_500)?;
    Ok(axum::Json(result))
}

#[derive(Deserialize)]
pub struct RequestRecoveryCodeParams {
    /// Whether or not the client is requesting a login link for a web-login. This is false for CLI logins.
    #[serde(default)]
    link: bool,
    email: String,
    identity: Identity,
}

pub async fn request_recovery_code(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Query(RequestRecoveryCodeParams {
        link,
        email,
        identity,
    }): Query<RequestRecoveryCodeParams>,
) -> axum::response::Result<impl IntoResponse> {
    let Some(sendgrid) = ctx.sendgrid_controller() else {
        log::error!("A recovery code was requested, but SendGrid is disabled.");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "SendGrid is disabled.").into());
    };

    if !ctx
        .control_db()
        .get_identities_for_email(email.as_str())
        .map_err(log_and_500)?
        .iter()
        .any(|a| a.identity == identity)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Email is not associated with the provided identity.",
        )
            .into());
    }

    let code = rand::thread_rng().gen_range(0..=999999);
    let code = format!("{code:06}");
    let recovery_code = RecoveryCode {
        code: code.clone(),
        generation_time: Utc::now(),
        identity: identity.to_hex(),
    };
    ctx.control_db()
        .spacetime_insert_recovery_code(email.as_str(), recovery_code)
        .await
        .map_err(log_and_500)?;

    sendgrid
        .send_recovery_email(email.as_str(), code.as_str(), &identity.to_hex(), link)
        .await
        .map_err(log_and_500)?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ConfirmRecoveryCodeParams {
    pub email: String,
    pub identity: Identity,
    pub code: String,
}

/// Note: We should be slightly more security conscious about this function because
///  we are providing a login token to the user initiating the request. We want to make
///  sure there aren't any logical issues in here that would allow a user to request a token
///  for an identity that they don't have authority over.
pub async fn confirm_recovery_code(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Query(ConfirmRecoveryCodeParams {
        email,
        identity,
        code,
    }): Query<ConfirmRecoveryCodeParams>,
) -> axum::response::Result<impl IntoResponse> {
    let recovery_code = ctx
        .control_db()
        .spacetime_get_recovery_code(email.as_str(), code.as_str())
        .await
        .map_err(log_and_500)?
        .ok_or((StatusCode::BAD_REQUEST, "Recovery code not found."))?;

    let duration = Utc::now() - recovery_code.generation_time;
    if duration.num_seconds() > 60 * 10 {
        return Err((StatusCode::BAD_REQUEST, "Recovery code expired.").into());
    }

    // Make sure the identity provided by the request matches the recovery code registration
    if recovery_code.identity != identity.to_hex() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Recovery code doesn't match the provided identity.",
        )
            .into());
    }

    if !ctx
        .control_db()
        .get_identities_for_email(email.as_str())
        .map_err(log_and_500)?
        .iter()
        .any(|a| a.identity == identity)
    {
        // This can happen if someone changes their associated email during a recovery request.
        return Err((
            StatusCode::BAD_REQUEST,
            "No identity associated with that email.",
        )
            .into());
    }

    // Recovery code is verified, return the identity and token to the user
    let token = encode_token(ctx.private_key(), identity).map_err(log_and_500)?;
    let result = RecoveryCodeResponse {
        identity: identity.to_hex(),
        token,
    };

    Ok(axum::Json(result))
}

async fn control_ctx_find_database(
    ctx: &dyn ControlCtx,
    address: &Address,
) -> Result<Option<Database>, StatusCode> {
    ctx.control_db()
        .get_database_by_address(address)
        .await
        .map_err(log_and_500)
}

#[derive(Deserialize)]
pub struct PublishDatabaseParams {}

#[derive(Deserialize)]
pub struct PublishDatabaseQueryParams {
    host_type: Option<String>,
    #[serde(default)]
    clear: bool,
    name_or_address: Option<NameOrAddress>,
    trace_log: Option<bool>,
    #[serde(default)]
    register_tld: bool,
}

#[cfg(not(feature = "tracelogging"))]
fn should_trace(_trace_log: Option<bool>) -> bool {
    false
}

#[cfg(feature = "tracelogging")]
fn should_trace(trace_log: Option<bool>) -> bool {
    trace_log.unwrap_or(false)
}

pub async fn publish(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(PublishDatabaseParams {}): Path<PublishDatabaseParams>,
    Query(query_params): Query<PublishDatabaseQueryParams>,
    auth: SpacetimeAuthHeader,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    let PublishDatabaseQueryParams {
        name_or_address,
        host_type,
        clear,
        trace_log,
        register_tld,
    } = query_params;

    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail.
    let auth = auth_or_bad_request(auth)?;

    let specified_address = matches!(name_or_address, Some(NameOrAddress::Address(_)));

    // Parse the address or convert the name to a usable address
    let db_address = if let Some(name_or_address) = name_or_address.clone() {
        match name_or_address.try_resolve(&*ctx).await? {
            Ok(address) => address,
            Err(name) => {
                let domain = name.parse().map_err(DomainParsingRejection)?;
                // Client specified a name which doesn't yet exist
                // Create a new DNS record and a new address to assign to it
                let address = ctx
                    .control_db()
                    .alloc_spacetime_address()
                    .await
                    .map_err(log_and_500)?;
                let result = ctx
                    .control_db()
                    .spacetime_insert_domain(&address, domain, auth.identity, register_tld)
                    .await
                    .map_err(log_and_500)?;
                match result {
                    InsertDomainResult::Success { .. } => {}
                    InsertDomainResult::TldNotRegistered { domain } => {
                        return Ok(axum::Json(PublishResult::TldNotRegistered { domain }))
                    }
                    InsertDomainResult::PermissionDenied { domain } => {
                        return Ok(axum::Json(PublishResult::PermissionDenied { domain }))
                    }
                }

                address
            }
        }
    } else {
        // No domain or address was specified, create a new one
        ctx.control_db()
            .alloc_spacetime_address()
            .await
            .map_err(log_and_500)?
    };

    log::trace!("Publishing to the address: {}", db_address.to_hex());

    let host_type = match host_type {
        None => HostType::Wasmer,
        Some(ht) => ht
            .parse()
            .map_err(|_| (StatusCode::BAD_REQUEST, format!("unknown host type {ht}")))?,
    };

    let program_bytes_addr = ctx.object_db().insert_object(body.into()).unwrap();

    let num_replicas = 1;

    let trace_log = should_trace(trace_log);

    let op = match control_ctx_find_database(&*ctx, &db_address).await? {
        Some(db) => {
            if Identity::from_slice(db.identity.as_slice()) != auth.identity {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Identity does not own this database.",
                )
                    .into());
            }

            if clear {
                ctx.insert_database(
                    &db_address,
                    &auth.identity,
                    &program_bytes_addr,
                    host_type,
                    num_replicas,
                    clear,
                    trace_log,
                )
                .await
                .map_err(log_and_500)?;
                PublishOp::Created
            } else {
                let res = ctx
                    .update_database(&db_address, &program_bytes_addr, num_replicas)
                    .await
                    .map_err(log_and_500)?;
                if let Some(res) = res {
                    let success = match res {
                        Ok(success) => success,
                        Err(e) => {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                format!("Database update rejected: {e}"),
                            )
                                .into());
                        }
                    };
                    if let UpdateDatabaseSuccess {
                        update_result: Some(update_result),
                        migrate_results: _,
                    } = success
                    {
                        match reducer_outcome_response(
                            &auth.identity,
                            "update",
                            update_result.outcome,
                        ) {
                            (StatusCode::OK, _) => {}
                            (status, body) => return Err((status, body).into()),
                        }
                    }
                }

                log::debug!("Updated database {}", db_address.to_hex());
                PublishOp::Updated
            }
        }
        None if specified_address => {
            return Err((
                StatusCode::NOT_FOUND,
                format!(
                    "Failed to find database at address: {}",
                    db_address.to_hex()
                ),
            )
                .into())
        }
        None => {
            ctx.insert_database(
                &db_address,
                &auth.identity,
                &program_bytes_addr,
                host_type,
                num_replicas,
                false,
                trace_log,
            )
            .await
            .map_err(log_and_500)?;
            PublishOp::Created
        }
    };

    let response = PublishResult::Success {
        domain: name_or_address.and_then(|noa| match noa {
            NameOrAddress::Address(_) => None,
            NameOrAddress::Name(name) => Some(name),
        }),
        address: db_address.to_hex(),
        op,
    };

    //TODO(tyler): Eventually we want it to be possible to publish a database
    // which no one has the credentials to. In that case we wouldn't want to
    // return a token.
    Ok(axum::Json(response))
}

#[derive(Deserialize)]
pub struct DeleteDatabaseParams {
    address: Address,
}

pub async fn delete_database(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(DeleteDatabaseParams { address }): Path<DeleteDatabaseParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth_or_bad_request(auth)?;

    match control_ctx_find_database(&*ctx, &address).await? {
        Some(db) => {
            if db.identity != auth.identity {
                Err((
                    StatusCode::BAD_REQUEST,
                    "Identity does not own this database.",
                )
                    .into())
            } else {
                ctx.delete_database(&address)
                    .await
                    .map_err(log_and_500)
                    .map_err(Into::into)
            }
        }
        None => Ok(()),
    }
}

#[derive(Deserialize)]
pub struct SetNameQueryParams {
    domain: String,
    address: Address,
    #[serde(default)]
    register_tld: bool,
}

pub async fn set_name(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Query(SetNameQueryParams {
        domain,
        address,
        register_tld,
    }): Query<SetNameQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth_or_bad_request(auth)?;

    let database = ctx
        .control_db()
        .get_database_by_address(&address)
        .await
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    if database.identity != auth.identity {
        return Err((StatusCode::BAD_REQUEST, "Identity does not own database.").into());
    }

    let domain = domain.parse().map_err(DomainParsingRejection)?;
    let response = ctx
        .control_db()
        .spacetime_insert_domain(&address, domain, auth.identity, register_tld)
        .await
        .map_err(log_and_500)?;

    Ok(axum::Json(response))
}

/// This API call is just designed to allow clients to determine whether or not they can
/// establish a connection to SpacetimeDB. This API call doesn't actually do anything.
pub async fn ping(
    State(_ctx): State<Arc<dyn ControlCtx>>,
    _auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    Ok(())
}

pub fn control_routes<S>() -> axum::Router<S>
where
    S: ControlNodeDelegate + Clone + 'static,
    Arc<dyn ControlCtx>: FromRef<S>,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/dns/:database_name", get(dns))
        .route("/reverse_dns/:database_address", get(reverse_dns))
        .route("/set_name", get(set_name))
        .route("/ping", get(ping))
        .route("/register_tld", get(register_tld))
        .route("/request_recovery_code", get(request_recovery_code))
        .route("/confirm_recovery_code", get(confirm_recovery_code))
        .route("/publish", post(publish).layer(DefaultBodyLimit::disable()))
        .route("/delete/:address", post(delete_database))
}

pub fn worker_routes<S>() -> axum::Router<S>
where
    S: ControlNodeDelegate + Clone + 'static,
    Arc<dyn WorkerCtx>: FromRef<S>,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/subscribe/:name_or_address",
            get(super::subscribe::handle_websocket),
        )
        .route("/call/:name_or_address/:reducer", post(call))
        .route(
            "/schema/:name_or_address/:entity_type/:entity",
            get(describe),
        )
        .route("/schema/:name_or_address", get(catalog))
        .route("/info/:name_or_address", get(info))
        .route("/logs/:name_or_address", get(logs))
        .route("/sql/:name_or_address", post(sql))
}
