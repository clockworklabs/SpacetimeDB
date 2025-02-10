use crate::auth::{
    anon_auth_middleware, SpacetimeAuth, SpacetimeAuthHeader, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros,
    SpacetimeIdentity, SpacetimeIdentityToken,
};
use crate::routes::subscribe::generate_random_connection_id;
use crate::util::{ByteStringBody, NameOrIdentity};
use crate::{log_and_500, ControlStateDelegate, DatabaseDef, NodeDelegate};
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::response::{ErrorResponse, IntoResponse};
use axum::Extension;
use axum_extra::TypedHeader;
use futures::StreamExt;
use http::StatusCode;
use serde::Deserialize;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::ReducerArgs;
use spacetimedb::host::ReducerCallError;
use spacetimedb::host::ReducerOutcome;
use spacetimedb::host::UpdateDatabaseResult;
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, HostType};
use spacetimedb_client_api_messages::name::{self, DnsLookupResponse, DomainName, PublishOp, PublishResult};
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::sats;

use super::identity::IdentityForUrl;

pub(crate) struct DomainParsingRejection;

impl IntoResponse for DomainParsingRejection {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, "Unable to parse domain name").into_response()
    }
}

#[derive(Deserialize)]
pub struct CallParams {
    name_or_identity: NameOrIdentity,
    reducer: String,
}

pub async fn call<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    Extension(auth): Extension<SpacetimeAuth>,
    Path(CallParams {
        name_or_identity,
        reducer,
    }): Path<CallParams>,
    ByteStringBody(body): ByteStringBody,
) -> axum::response::Result<impl IntoResponse> {
    let caller_identity = auth.identity;

    let args = ReducerArgs::Json(body);

    let db_identity = name_or_identity.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", db_identity.to_hex());
            (StatusCode::NOT_FOUND, "No such database.")
        })?;
    let identity = database.owner_identity;

    let leader = worker_ctx
        .leader(database.id)
        .await
        .map_err(log_and_500)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let module = leader.module().await.map_err(log_and_500)?;

    // HTTP callers always need a connection ID to provide to connect/disconnect,
    // so generate one.
    let connection_id = generate_random_connection_id();

    if let Err(e) = module
        .call_identity_connected_disconnected(caller_identity, connection_id, true)
        .await
    {
        return Err((StatusCode::NOT_FOUND, format!("{:#}", anyhow::anyhow!(e))).into());
    }
    let result = match module
        .call_reducer(caller_identity, Some(connection_id), None, None, None, &reducer, args)
        .await
    {
        Ok(rcr) => Ok(rcr),
        Err(e) => {
            let status_code = match e {
                ReducerCallError::Args(_) => {
                    log::debug!("Attempt to call reducer with invalid arguments");
                    StatusCode::BAD_REQUEST
                }
                ReducerCallError::NoSuchModule(_) | ReducerCallError::ScheduleReducerNotFound => StatusCode::NOT_FOUND,
                ReducerCallError::NoSuchReducer => {
                    log::debug!("Attempt to call non-existent reducer {}", reducer);
                    StatusCode::NOT_FOUND
                }
                ReducerCallError::LifecycleReducer(lifecycle) => {
                    log::debug!("Attempt to call {lifecycle:?} lifeycle reducer {}", reducer);
                    StatusCode::BAD_REQUEST
                }
            };

            log::debug!("Error while invoking reducer {:#}", e);
            Err((status_code, format!("{:#}", anyhow::anyhow!(e))))
        }
    };

    if let Err(e) = module
        .call_identity_connected_disconnected(caller_identity, connection_id, false)
        .await
    {
        return Err((StatusCode::NOT_FOUND, format!("{:#}", anyhow::anyhow!(e))).into());
    }

    match result {
        Ok(result) => {
            let (status, body) = reducer_outcome_response(&identity, &reducer, result.outcome);
            Ok((
                status,
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

#[derive(Deserialize)]
pub struct SchemaParams {
    name_or_identity: NameOrIdentity,
}
#[derive(Deserialize)]
pub struct SchemaQueryParams {
    version: SchemaVersion,
}

#[derive(Deserialize)]
enum SchemaVersion {
    #[serde(rename = "9")]
    V9,
}

pub async fn schema<S>(
    State(worker_ctx): State<S>,
    Path(SchemaParams { name_or_identity }): Path<SchemaParams>,
    Query(SchemaQueryParams { version }): Query<SchemaQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    let db_identity = name_or_identity.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let leader = worker_ctx
        .leader(database.id)
        .await
        .map_err(log_and_500)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let module = leader.module().await.map_err(log_and_500)?;

    let module_def = &module.info.module_def;
    let response_json = match version {
        SchemaVersion::V9 => {
            let raw = RawModuleDefV9::from(module_def.clone());
            axum::Json(sats::serde::SerdeWrapper(raw)).into_response()
        }
    };

    Ok((
        TypedHeader(SpacetimeIdentity(auth.identity)),
        TypedHeader(SpacetimeIdentityToken(auth.creds)),
        response_json,
    ))
}

#[derive(Deserialize)]
pub struct DatabaseParam {
    name_or_identity: NameOrIdentity,
}

#[derive(sats::Serialize)]
struct DatabaseResponse {
    database_identity: Identity,
    owner_identity: Identity,
    host_type: HostType,
    initial_program: spacetimedb_lib::Hash,
}

impl From<Database> for DatabaseResponse {
    fn from(db: Database) -> Self {
        DatabaseResponse {
            database_identity: db.database_identity,
            owner_identity: db.owner_identity,
            host_type: db.host_type,
            initial_program: db.initial_program,
        }
    }
}

pub async fn db_info<S: ControlStateDelegate>(
    State(worker_ctx): State<S>,
    Path(DatabaseParam { name_or_identity }): Path<DatabaseParam>,
) -> axum::response::Result<impl IntoResponse> {
    log::trace!("Trying to resolve database identity: {:?}", name_or_identity);
    let database_identity = name_or_identity.resolve(&worker_ctx).await?.into();
    log::trace!("Resolved identity to: {database_identity:?}");
    let database = worker_ctx_find_database(&worker_ctx, &database_identity)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;
    log::trace!("Fetched database from the worker db for database identity: {database_identity:?}");

    let response = DatabaseResponse::from(database);
    Ok(axum::Json(sats::serde::SerdeWrapper(response)))
}

#[derive(Deserialize)]
pub struct LogsParams {
    name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    num_lines: Option<u32>,
    #[serde(default)]
    follow: bool,
}

pub async fn logs<S>(
    State(worker_ctx): State<S>,
    Path(LogsParams { name_or_identity }): Path<LogsParams>,
    Query(LogsQuery { num_lines, follow }): Query<LogsQuery>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    // You should not be able to read the logs from a database that you do not own
    // so, unless you are the owner, this will fail.

    let database_identity: Identity = name_or_identity.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &database_identity)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    if database.owner_identity != auth.identity {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Identity does not own database, expected: {} got: {}",
                database.owner_identity.to_hex(),
                auth.identity.to_hex()
            ),
        )
            .into());
    }

    let replica = worker_ctx
        .get_leader_replica_by_database(database.id)
        .ok_or((StatusCode::NOT_FOUND, "Replica not scheduled to this node yet."))?;
    let replica_id = replica.id;

    let logs_dir = worker_ctx.module_logs_dir(replica_id);
    let lines = DatabaseLogger::read_latest(logs_dir, num_lines).await;

    let body = if follow {
        let leader = worker_ctx
            .leader(database.id)
            .await
            .map_err(log_and_500)?
            .ok_or(StatusCode::NOT_FOUND)?;
        let log_rx = leader
            .module()
            .await
            .map_err(log_and_500)?
            .subscribe_to_logs()
            .map_err(log_and_500)?;

        let stream = tokio_stream::wrappers::BroadcastStream::new(log_rx).filter_map(move |x| {
            std::future::ready(match x {
                Ok(log) => Some(log),
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped)) => {
                    log::trace!(
                        "Skipped {} lines in log for module {}",
                        skipped,
                        database_identity.to_hex()
                    );
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
    database_identity: &Identity,
) -> axum::response::Result<Option<Database>> {
    worker_ctx
        .get_database_by_identity(database_identity)
        .map_err(log_and_500)
}

#[derive(Deserialize)]
pub struct SqlParams {
    name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct SqlQueryParams {}

pub async fn sql<S>(
    State(worker_ctx): State<S>,
    Path(SqlParams { name_or_identity }): Path<SqlParams>,
    Query(SqlQueryParams {}): Query<SqlQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: String,
) -> axum::response::Result<impl IntoResponse>
where
    S: NodeDelegate + ControlStateDelegate,
{
    // Anyone is authorized to execute SQL queries. The SQL engine will determine
    // which queries this identity is allowed to execute against the database.

    let db_identity = name_or_identity.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let auth = AuthCtx::new(database.owner_identity, auth.identity);
    log::debug!("auth: {auth:?}");

    let host = worker_ctx
        .leader(database.id)
        .await
        .map_err(log_and_500)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let json = host.exec_sql(auth, database, body).await?;

    Ok(axum::Json(json))
}

#[derive(Deserialize)]
pub struct DNSParams {
    database_name: String,
}

#[derive(Deserialize)]
pub struct ReverseDNSParams {
    database_identity: IdentityForUrl,
}

#[derive(Deserialize)]
pub struct DNSQueryParams {}

pub async fn dns<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DNSParams { database_name }): Path<DNSParams>,
    Query(DNSQueryParams {}): Query<DNSQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let domain = database_name.parse().map_err(|_| DomainParsingRejection)?;
    let db_identity = ctx.lookup_identity(&domain).map_err(log_and_500)?;
    let response = if let Some(db_identity) = db_identity {
        DnsLookupResponse::Success {
            domain,
            identity: db_identity,
        }
    } else {
        DnsLookupResponse::Failure { domain }
    };

    Ok(axum::Json(response))
}

pub async fn reverse_dns<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(ReverseDNSParams { database_identity }): Path<ReverseDNSParams>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = Identity::from(database_identity);

    let names = ctx.reverse_lookup(&database_identity).map_err(log_and_500)?;

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
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail, hence not using get_or_create

    let tld = tld.parse::<DomainName>().map_err(|_| DomainParsingRejection)?.into();
    let result = ctx.register_tld(&auth.identity, tld).await.map_err(log_and_500)?;
    Ok(axum::Json(result))
}

#[derive(Deserialize)]
pub struct PublishDatabaseParams {}

#[derive(Deserialize)]
pub struct PublishDatabaseQueryParams {
    #[serde(default)]
    clear: bool,
    name_or_identity: Option<NameOrIdentity>,
}

impl PublishDatabaseQueryParams {
    pub fn name_or_identity(&self) -> Option<&NameOrIdentity> {
        self.name_or_identity.as_ref()
    }
}

pub async fn publish<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams {}): Path<PublishDatabaseParams>,
    Query(query_params): Query<PublishDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    let PublishDatabaseQueryParams {
        name_or_identity,
        clear,
    } = query_params;

    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail.

    let (database_identity, db_name) = match name_or_identity {
        Some(noa) => match noa.try_resolve(&ctx).await? {
            Ok(resolved) => resolved.into(),
            Err(domain) => {
                // `name_or_identity` was a `NameOrIdentity::Name`, but no record
                // exists yet. Create it now with a fresh identity.
                let database_auth = SpacetimeAuth::alloc(&ctx).await?;
                let database_identity = database_auth.identity;
                ctx.create_dns_record(&auth.identity, &domain, &database_identity)
                    .await
                    .map_err(log_and_500)?;
                (database_identity, Some(domain))
            }
        },
        None => {
            let database_auth = SpacetimeAuth::alloc(&ctx).await?;
            let database_identity = database_auth.identity;
            (database_identity, None)
        }
    };

    log::trace!("Publishing to the identity: {}", database_identity.to_hex());

    let op = {
        let exists = ctx
            .get_database_by_identity(&database_identity)
            .map_err(log_and_500)?
            .is_some();

        if clear && exists {
            ctx.delete_database(&auth.identity, &database_identity)
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
            DatabaseDef {
                database_identity,
                program_bytes: body.into(),
                num_replicas: 1,
                host_type: HostType::Wasm,
            },
        )
        .await
        .map_err(log_and_500)?;

    if let Some(updated) = maybe_updated {
        match updated {
            UpdateDatabaseResult::AutoMigrateError(errs) => {
                return Err((StatusCode::BAD_REQUEST, format!("Database update rejected: {errs}")).into());
            }
            UpdateDatabaseResult::ErrorExecutingMigration(err) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Failed to create or update the database: {err}"),
                )
                    .into());
            }
            UpdateDatabaseResult::NoUpdateNeeded | UpdateDatabaseResult::UpdatePerformed => {}
        }
    }

    Ok(axum::Json(PublishResult::Success {
        domain: db_name.as_ref().map(ToString::to_string),
        database_identity,
        op,
    }))
}

#[derive(Deserialize)]
pub struct DeleteDatabaseParams {
    database_identity: IdentityForUrl,
}

pub async fn delete_database<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DeleteDatabaseParams { database_identity }): Path<DeleteDatabaseParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = Identity::from(database_identity);

    ctx.delete_database(&auth.identity, &database_identity)
        .await
        .map_err(log_and_500)?;

    Ok(())
}

#[derive(Deserialize)]
pub struct SetNameQueryParams {
    domain: String,
    database_identity: IdentityForUrl,
}

pub async fn set_name<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Query(SetNameQueryParams {
        domain,
        database_identity,
    }): Query<SetNameQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = Identity::from(database_identity);

    let database = ctx
        .get_database_by_identity(&database_identity)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    if database.owner_identity != auth.identity {
        return Err((StatusCode::UNAUTHORIZED, "Identity does not own database.").into());
    }

    let domain = domain.parse().map_err(|_| DomainParsingRejection)?;
    let response = ctx
        .create_dns_record(&auth.identity, &domain, &database_identity)
        .await
        // TODO: better error code handling
        .map_err(log_and_500)?;

    Ok(axum::Json(response))
}

/// This API call is just designed to allow clients to determine whether or not they can
/// establish a connection to SpacetimeDB. This API call doesn't actually do anything.
pub async fn ping<S>(State(_ctx): State<S>, _auth: SpacetimeAuthHeader) -> axum::response::Result<impl IntoResponse> {
    Ok(())
}

pub fn control_routes<S>(ctx: S) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/dns/:database_name", get(dns::<S>))
        .route("/reverse_dns/:database_identity", get(reverse_dns::<S>))
        .route("/set_name", get(set_name::<S>))
        .route("/ping", get(ping::<S>))
        .route("/register_tld", get(register_tld::<S>))
        .route("/publish", post(publish::<S>).layer(DefaultBodyLimit::disable()))
        .route("/delete/:database_identity", post(delete_database::<S>))
        .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
}

pub fn worker_routes<S>(ctx: S) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/:name_or_identity", get(db_info::<S>))
        .route(
            "/subscribe/:name_or_identity",
            get(super::subscribe::handle_websocket::<S>),
        )
        .route("/call/:name_or_identity/:reducer", post(call::<S>))
        .route("/schema/:name_or_identity", get(schema::<S>))
        .route("/logs/:name_or_identity", get(logs::<S>))
        .route("/sql/:name_or_identity", post(sql::<S>))
        .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
}
