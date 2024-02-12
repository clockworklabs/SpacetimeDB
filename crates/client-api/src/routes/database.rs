use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::response::{ErrorResponse, IntoResponse};
use axum_extra::TypedHeader;
use chrono::Utc;
use futures::StreamExt;
use http::StatusCode;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use spacetimedb::address::Address;
use spacetimedb::auth::identity::encode_token;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::DescribedEntityType;
use spacetimedb::host::EntityDef;
use spacetimedb::host::ReducerArgs;
use spacetimedb::host::ReducerCallError;
use spacetimedb::host::ReducerOutcome;
use spacetimedb::host::UpdateDatabaseSuccess;
use spacetimedb::identity::Identity;
use spacetimedb::json::client_api::StmtResultJson;
use spacetimedb::messages::control_db::{Database, DatabaseInstance};
use spacetimedb::sql::execute::execute;
use spacetimedb_lib::address::AddressForUrl;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::name::{self, DnsLookupResponse, DomainName, DomainParsingError, PublishOp, PublishResult};
use spacetimedb_lib::recovery::{RecoveryCode, RecoveryCodeResponse};
use spacetimedb_lib::sats::WithTypespace;
use std::collections::HashMap;
use std::convert::From;

use super::identity::IdentityForUrl;
use crate::auth::{
    SpacetimeAuth, SpacetimeAuthHeader, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros, SpacetimeIdentity,
    SpacetimeIdentityToken,
};
use crate::routes::subscribe::generate_random_address;
use crate::util::{ByteStringBody, NameOrAddress};
use crate::{log_and_500, ControlStateDelegate, DatabaseDef, NodeDelegate};

#[derive(derive_more::From)]
pub(crate) struct DomainParsingRejection(pub(crate) DomainParsingError);

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

#[derive(Deserialize)]
pub struct CallQueryParams {
    client_address: Option<AddressForUrl>,
}

pub async fn call<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    auth: SpacetimeAuthHeader,
    Path(CallParams {
        name_or_address,
        reducer,
    }): Path<CallParams>,
    Query(CallQueryParams { client_address }): Query<CallQueryParams>,
    ByteStringBody(body): ByteStringBody,
) -> axum::response::Result<impl IntoResponse> {
    let SpacetimeAuth {
        identity: caller_identity,
        creds: caller_identity_token,
    } = auth.get_or_create(&worker_ctx).await?;

    let args = ReducerArgs::Json(body);

    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address).await?.ok_or_else(|| {
        log::error!("Could not find database: {}", address.to_hex());
        (StatusCode::NOT_FOUND, "No such database.")
    })?;
    let identity = database.identity;
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
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

    // HTTP callers always need an address to provide to connect/disconnect,
    // so generate one if none was provided.
    let client_address = client_address
        .map(Address::from)
        .unwrap_or_else(generate_random_address);

    if let Err(e) = module
        .call_identity_connected_disconnected(caller_identity, client_address, true)
        .await
    {
        return Err((StatusCode::NOT_FOUND, format!("{:#}", anyhow::anyhow!(e))).into());
    }
    let result = match module
        .call_reducer(caller_identity, Some(client_address), None, &reducer, args)
        .await
    {
        Ok(rcr) => Ok(rcr),
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
            Err((status_code, format!("{:#}", anyhow::anyhow!(e))))
        }
    };

    if let Err(e) = module
        .call_identity_connected_disconnected(caller_identity, client_address, false)
        .await
    {
        return Err((StatusCode::NOT_FOUND, format!("{:#}", anyhow::anyhow!(e))).into());
    }

    match result {
        Ok(result) => {
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
        Err(e) => Err((e.0, e.1).into()),
    }
}

fn reducer_outcome_response(identity: &Identity, reducer: &str, outcome: ReducerOutcome) -> (StatusCode, String) {
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

#[derive(Debug, derive_more::From)]
pub enum DBCallErr {
    HandlerError(ErrorResponse),
    NoSuchDatabase,
    InstanceNotScheduled,
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
    ctx: &(impl ControlStateDelegate + NodeDelegate + ?Sized),
    auth: SpacetimeAuthHeader,
    address: &Address,
) -> Result<DatabaseInformation, ErrorResponse> {
    let auth = auth.get_or_create(ctx).await?;

    let database = worker_ctx_find_database(ctx, address).await?.ok_or_else(|| {
        log::error!("Could not find database: {}", address.to_hex());
        (StatusCode::NOT_FOUND, "No such database.")
    })?;

    let database_instance = ctx.get_leader_database_instance_by_database(database.id).ok_or((
        StatusCode::NOT_FOUND,
        "Database instance not scheduled to this node yet.",
    ))?;

    Ok(DatabaseInformation {
        database_instance,
        auth,
    })
}

fn entity_description_json(description: WithTypespace<EntityDef>, expand: bool) -> Option<Value> {
    let typ = DescribedEntityType::from_entitydef(description.ty()).as_str();
    let len = match description.ty() {
        EntityDef::Table(t) => description.resolve(t.data).ty().as_product()?.elements.len(),
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

pub async fn describe<S>(
    State(worker_ctx): State<S>,
    Path(DescribeParams {
        name_or_address,
        entity_type,
        entity,
    }): Path<DescribeParams>,
    Query(DescribeQueryParams { expand }): Query<DescribeQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let call_info = extract_db_call_info(&worker_ctx, auth, &address).await?;

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
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("{entity_type} {entity:?} not found")))?;

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
pub async fn catalog<S>(
    State(worker_ctx): State<S>,
    Path(CatalogParams { name_or_address }): Path<CatalogParams>,
    Query(DescribeQueryParams { expand }): Query<DescribeQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let call_info = extract_db_call_info(&worker_ctx, auth, &address).await?;

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
pub async fn info<S: ControlStateDelegate>(
    State(worker_ctx): State<S>,
    Path(InfoParams { name_or_address }): Path<InfoParams>,
) -> axum::response::Result<impl IntoResponse> {
    log::trace!("Trying to resolve address: {:?}", name_or_address);
    let address = name_or_address.resolve(&worker_ctx).await?.into();
    log::trace!("Resolved address to: {address:?}");
    let database = worker_ctx_find_database(&worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;
    log::trace!("Fetched database from the worker db for address: {address:?}");

    let host_type: &str = database.host_type.as_ref();
    let response_json = json!({
        "address": database.address,
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

pub async fn logs<S>(
    State(worker_ctx): State<S>,
    Path(LogsParams { name_or_address }): Path<LogsParams>,
    Query(LogsQuery { num_lines, follow }): Query<LogsQuery>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    // You should not be able to read the logs from a database that you do not own
    // so, unless you are the owner, this will fail.
    // TODO: This returns `UNAUTHORIZED` on failure,
    //       while everywhere else we return `BAD_REQUEST`.
    //       Is this special in some way? Should this change?
    //       Should all the others change?
    let auth = auth_or_unauth(auth)?;

    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
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
                    log::trace!("Skipped {} lines in log for module {}", skipped, address.to_hex());
                    None
                }
            })
        });

        let stream = futures::stream::once(std::future::ready(lines.into()))
            .chain(stream)
            .map(Ok::<_, std::convert::Infallible>);

        Body::from_stream(stream)
    } else {
        Body::from(lines)
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
    worker_ctx: &(impl ControlStateDelegate + ?Sized),
    address: &Address,
) -> axum::response::Result<Option<Database>> {
    worker_ctx.get_database_by_address(address).map_err(log_and_500)
}

#[derive(Deserialize)]
pub struct SqlParams {
    name_or_address: NameOrAddress,
}

#[derive(Deserialize)]
pub struct SqlQueryParams {}

pub async fn sql<S>(
    State(worker_ctx): State<S>,
    Path(SqlParams { name_or_address }): Path<SqlParams>,
    Query(SqlQueryParams {}): Query<SqlQueryParams>,
    auth: SpacetimeAuthHeader,
    body: String,
) -> axum::response::Result<impl IntoResponse>
where
    S: NodeDelegate + ControlStateDelegate,
{
    // Anyone is authorized to execute SQL queries. The SQL engine will determine
    // which queries this identity is allowed to execute against the database.
    let auth = auth.get().ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials."))?;

    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let auth = AuthCtx::new(database.identity, auth.identity);
    log::debug!("auth: {auth:?}");
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
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
            rows: result.data.into_iter().map(|x| x.data.elements).collect::<Vec<_>>(),
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
    database_address: AddressForUrl,
}

#[derive(Deserialize)]
pub struct DNSQueryParams {}

pub async fn dns<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DNSParams { database_name }): Path<DNSParams>,
    Query(DNSQueryParams {}): Query<DNSQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let domain = database_name.parse().map_err(DomainParsingRejection)?;
    let address = ctx.lookup_address(&domain).map_err(log_and_500)?;
    let response = if let Some(address) = address {
        DnsLookupResponse::Success { domain, address }
    } else {
        DnsLookupResponse::Failure { domain }
    };

    Ok(axum::Json(response))
}

pub async fn reverse_dns<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(ReverseDNSParams { database_address }): Path<ReverseDNSParams>,
) -> axum::response::Result<impl IntoResponse> {
    let database_address = Address::from(database_address);

    let names = ctx.reverse_lookup(&database_address).map_err(log_and_500)?;

    let response = name::ReverseDNSResponse { names };
    Ok(axum::Json(response))
}

#[derive(Deserialize)]
pub struct RegisterTldParams {
    tld: String,
}

pub async fn register_tld<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Query(RegisterTldParams { tld }): Query<RegisterTldParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail, hence not using get_or_create
    let auth = auth_or_unauth(auth)?;

    let tld = tld.parse::<DomainName>().map_err(DomainParsingRejection)?.into();
    let result = ctx.register_tld(&auth.identity, tld).await.map_err(log_and_500)?;
    Ok(axum::Json(result))
}

#[derive(Deserialize)]
pub struct RequestRecoveryCodeParams {
    /// Whether or not the client is requesting a login link for a web-login. This is false for CLI logins.
    #[serde(default)]
    link: bool,
    email: String,
    identity: IdentityForUrl,
}

pub async fn request_recovery_code<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Query(RequestRecoveryCodeParams { link, email, identity }): Query<RequestRecoveryCodeParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
    let Some(sendgrid) = ctx.sendgrid_controller() else {
        log::error!("A recovery code was requested, but SendGrid is disabled.");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "SendGrid is disabled.").into());
    };

    if !ctx
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
        identity,
    };
    ctx.insert_recovery_code(&identity, email.as_str(), recovery_code)
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
    pub identity: IdentityForUrl,
    pub code: String,
}

/// Note: We should be slightly more security conscious about this function because
///  we are providing a login token to the user initiating the request. We want to make
///  sure there aren't any logical issues in here that would allow a user to request a token
///  for an identity that they don't have authority over.
pub async fn confirm_recovery_code<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
    Query(ConfirmRecoveryCodeParams { email, identity, code }): Query<ConfirmRecoveryCodeParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
    let recovery_codes = ctx.get_recovery_codes(email.as_str()).map_err(log_and_500)?;

    let recovery_code = recovery_codes
        .into_iter()
        .find(|rc| rc.code == code.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Recovery code not found."))?;

    let duration = Utc::now() - recovery_code.generation_time;
    if duration.num_seconds() > 60 * 10 {
        return Err((StatusCode::BAD_REQUEST, "Recovery code expired.").into());
    }

    // Make sure the identity provided by the request matches the recovery code registration
    if recovery_code.identity != identity {
        return Err((
            StatusCode::BAD_REQUEST,
            "Recovery code doesn't match the provided identity.",
        )
            .into());
    }

    if !ctx
        .get_identities_for_email(email.as_str())
        .map_err(log_and_500)?
        .iter()
        .any(|a| a.identity == identity)
    {
        // This can happen if someone changes their associated email during a recovery request.
        return Err((StatusCode::BAD_REQUEST, "No identity associated with that email.").into());
    }

    // Recovery code is verified, return the identity and token to the user
    let token = encode_token(ctx.private_key(), identity).map_err(log_and_500)?;
    let result = RecoveryCodeResponse { identity, token };

    Ok(axum::Json(result))
}

#[derive(Deserialize)]
pub struct PublishDatabaseParams {}

#[derive(Deserialize)]
pub struct PublishDatabaseQueryParams {
    #[serde(default)]
    clear: bool,
    name_or_address: Option<NameOrAddress>,
    client_address: Option<AddressForUrl>,
}

pub async fn publish<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams {}): Path<PublishDatabaseParams>,
    Query(query_params): Query<PublishDatabaseQueryParams>,
    auth: SpacetimeAuthHeader,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    let PublishDatabaseQueryParams {
        name_or_address,
        clear,
        client_address,
    } = query_params;

    let client_address = client_address.map(Address::from);

    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail.
    let auth = auth_or_unauth(auth)?;

    let (db_addr, db_name) = match name_or_address {
        Some(noa) => match noa.try_resolve(&ctx).await? {
            Ok(resolved) => resolved.into(),
            Err(domain) => {
                // `name_or_address` was a `NameOrAddress::Name`, but no record
                // exists yet. Create it now with a fresh address.
                let addr = ctx.create_address().await.map_err(log_and_500)?;
                ctx.create_dns_record(&auth.identity, &domain, &addr)
                    .await
                    .map_err(log_and_500)?;
                (addr, Some(domain))
            }
        },
        None => {
            let addr = ctx.create_address().await.map_err(log_and_500)?;
            (addr, None)
        }
    };

    log::trace!("Publishing to the address: {}", db_addr.to_hex());

    let op = {
        let exists = ctx.get_database_by_address(&db_addr).map_err(log_and_500)?.is_some();

        if clear && exists {
            ctx.delete_database(&auth.identity, &db_addr)
                .await
                .map_err(log_and_500)?;
        }

        if exists {
            PublishOp::Updated
        } else {
            PublishOp::Created
        }
    };

    let maybe_updated = ctx
        .publish_database(
            &auth.identity,
            client_address,
            DatabaseDef {
                address: db_addr,
                program_bytes: body.into(),
                num_replicas: 1,
            },
        )
        .await
        .map_err(log_and_500)?;

    if let Some(updated) = maybe_updated {
        match updated {
            Ok(success) => {
                if let UpdateDatabaseSuccess {
                    // An update reducer was defined, and it was run
                    update_result: Some(update_result),
                    // Not yet implemented
                    migrate_results: _,
                } = success
                {
                    let ror = reducer_outcome_response(&auth.identity, "update", update_result.outcome);
                    if !matches!(ror, (StatusCode::OK, _)) {
                        return Err(ror.into());
                    }
                }
            }
            Err(e) => return Err((StatusCode::BAD_REQUEST, format!("Database update rejected: {e}")).into()),
        }
    }

    Ok(axum::Json(PublishResult::Success {
        domain: db_name.as_ref().map(ToString::to_string),
        address: db_addr,
        op,
    }))
}

#[derive(Deserialize)]
pub struct DeleteDatabaseParams {
    address: AddressForUrl,
}

pub async fn delete_database<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DeleteDatabaseParams { address }): Path<DeleteDatabaseParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth_or_unauth(auth)?;

    let address = Address::from(address);

    ctx.delete_database(&auth.identity, &address)
        .await
        .map_err(log_and_500)?;

    Ok(())
}

#[derive(Deserialize)]
pub struct SetNameQueryParams {
    domain: String,
    address: AddressForUrl,
}

pub async fn set_name<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Query(SetNameQueryParams { domain, address }): Query<SetNameQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth_or_unauth(auth)?;

    let address = Address::from(address);

    let database = ctx
        .get_database_by_address(&address)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    if database.identity != auth.identity {
        return Err((StatusCode::UNAUTHORIZED, "Identity does not own database.").into());
    }

    let domain = domain.parse().map_err(DomainParsingRejection)?;
    let response = ctx
        .create_dns_record(&auth.identity, &domain, &address)
        .await
        .map_err(|err| match err {
            spacetimedb::control_db::Error::RecordAlreadyExists(_) => StatusCode::CONFLICT.into(),
            _ => log_and_500(err),
        })?;

    Ok(axum::Json(response))
}

/// This API call is just designed to allow clients to determine whether or not they can
/// establish a connection to SpacetimeDB. This API call doesn't actually do anything.
pub async fn ping<S>(State(_ctx): State<S>, _auth: SpacetimeAuthHeader) -> axum::response::Result<impl IntoResponse> {
    Ok(())
}

pub fn control_routes<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/dns/:database_name", get(dns::<S>))
        .route("/reverse_dns/:database_address", get(reverse_dns::<S>))
        .route("/set_name", get(set_name::<S>))
        .route("/ping", get(ping::<S>))
        .route("/register_tld", get(register_tld::<S>))
        .route("/request_recovery_code", get(request_recovery_code::<S>))
        .route("/confirm_recovery_code", get(confirm_recovery_code::<S>))
        .route("/publish", post(publish::<S>).layer(DefaultBodyLimit::disable()))
        .route("/delete/:address", post(delete_database::<S>))
}

pub fn worker_routes<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/subscribe/:name_or_address",
            get(super::subscribe::handle_websocket::<S>),
        )
        .route("/call/:name_or_address/:reducer", post(call::<S>))
        .route("/schema/:name_or_address/:entity_type/:entity", get(describe::<S>))
        .route("/schema/:name_or_address", get(catalog::<S>))
        .route("/info/:name_or_address", get(info::<S>))
        .route("/logs/:name_or_address", get(logs::<S>))
        .route("/sql/:name_or_address", post(sql::<S>))
}
