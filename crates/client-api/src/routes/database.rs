use std::num::NonZeroU8;
use std::str::FromStr;
use std::time::Duration;

use crate::auth::{
    anon_auth_middleware, SpacetimeAuth, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros, SpacetimeIdentity,
    SpacetimeIdentityToken,
};
use crate::routes::subscribe::generate_random_connection_id;
use crate::util::{ByteStringBody, NameOrIdentity};
use crate::{log_and_500, ControlStateDelegate, DatabaseDef, NodeDelegate};
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::response::{ErrorResponse, IntoResponse};
use axum::routing::MethodRouter;
use axum::Extension;
use axum_extra::TypedHeader;
use futures::StreamExt;
use http::StatusCode;
use serde::Deserialize;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::module_host::ClientConnectedError;
use spacetimedb::host::ReducerArgs;
use spacetimedb::host::ReducerCallError;
use spacetimedb::host::ReducerOutcome;
use spacetimedb::host::UpdateDatabaseResult;
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, HostType};
use spacetimedb_client_api_messages::name::{self, DatabaseName, DomainName, PublishOp, PublishResult};
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::sats;

use super::subscribe::handle_websocket;

#[derive(Deserialize)]
pub struct CallParams {
    name_or_identity: NameOrIdentity,
    reducer: String,
}

pub const NO_SUCH_DATABASE: (StatusCode, &str) = (StatusCode::NOT_FOUND, "No such database.");

pub async fn call<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    Extension(auth): Extension<SpacetimeAuth>,
    Path(CallParams {
        name_or_identity,
        reducer,
    }): Path<CallParams>,
    TypedHeader(content_type): TypedHeader<headers::ContentType>,
    ByteStringBody(body): ByteStringBody,
) -> axum::response::Result<impl IntoResponse> {
    if content_type != headers::ContentType::json() {
        return Err(axum::extract::rejection::MissingJsonContentType::default().into());
    }
    let caller_identity = auth.identity;

    let args = ReducerArgs::Json(body);

    let db_identity = name_or_identity.resolve(&worker_ctx).await?;
    let database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", db_identity.to_hex());
            NO_SUCH_DATABASE
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

    match module.call_identity_connected(caller_identity, connection_id).await {
        // If `call_identity_connected` returns `Err(Rejected)`, then the `client_connected` reducer errored,
        // meaning the connection was refused. Return 403 forbidden.
        Err(ClientConnectedError::Rejected(msg)) => return Err((StatusCode::FORBIDDEN, msg).into()),
        // If `call_identity_connected` returns `Err(OutOfEnergy)`,
        // then, well, the database is out of energy.
        // Return 503 service unavailable.
        Err(err @ ClientConnectedError::OutOfEnergy) => {
            return Err((StatusCode::SERVICE_UNAVAILABLE, err.to_string()).into())
        }
        // If `call_identity_connected` returns `Err(ReducerCall)`,
        // something went wrong while invoking the `client_connected` reducer.
        // I (pgoldman 2025-03-27) am not really sure how this would happen,
        // but we returned 404 not found in this case prior to my editing this code,
        // so I guess let's keep doing that.
        Err(ClientConnectedError::ReducerCall(e)) => {
            return Err((StatusCode::NOT_FOUND, format!("{:#}", anyhow::anyhow!(e))).into())
        }
        // If `call_identity_connected` returns `Err(DBError)`,
        // then the module didn't define `client_connected`,
        // but something went wrong when we tried to insert into `st_client`.
        // That's weird and scary, so return 500 internal error.
        Err(e @ ClientConnectedError::DBError(_)) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into())
        }

        // If `call_identity_connected` returns `Ok`, then we can actually call the reducer we want.
        Ok(()) => (),
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

    if let Err(e) = module.call_identity_disconnected(caller_identity, connection_id).await {
        // If `call_identity_disconnected` errors, something is very wrong:
        // it means we tried to delete the `st_client` row but failed.
        // Note that `call_identity_disconnected` swallows errors from the `client_disconnected` reducer.
        // Slap a 500 on it and pray.
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("{:#}", anyhow::anyhow!(e))).into());
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
    let db_identity = name_or_identity.resolve(&worker_ctx).await?;
    let database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;

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
    let database_identity = name_or_identity.resolve(&worker_ctx).await?;
    log::trace!("Resolved identity to: {database_identity:?}");
    let database = worker_ctx_find_database(&worker_ctx, &database_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;
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

    let database_identity: Identity = name_or_identity.resolve(&worker_ctx).await?;
    let database = worker_ctx_find_database(&worker_ctx, &database_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;

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

    let db_identity = name_or_identity.resolve(&worker_ctx).await?;
    let database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;

    let auth = AuthCtx::new(database.owner_identity, auth.identity);
    log::debug!("auth: {auth:?}");

    let host = worker_ctx
        .leader(database.id)
        .await
        .map_err(log_and_500)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let json = host.exec_sql(auth, database, body).await?;

    let total_duration = json.iter().fold(0, |acc, x| acc + x.total_duration_micros);

    Ok((
        TypedHeader(SpacetimeExecutionDurationMicros(Duration::from_micros(total_duration))),
        axum::Json(json),
    ))
}

#[derive(Deserialize)]
pub struct DNSParams {
    name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct ReverseDNSParams {
    name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct DNSQueryParams {}

pub async fn get_identity<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DNSParams { name_or_identity }): Path<DNSParams>,
    Query(DNSQueryParams {}): Query<DNSQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = name_or_identity.resolve(&ctx).await?;
    Ok(identity.to_string())
}

pub async fn get_names<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(ReverseDNSParams { name_or_identity }): Path<ReverseDNSParams>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = name_or_identity.resolve(&ctx).await?;

    let names = ctx
        .reverse_lookup(&database_identity)
        .map_err(log_and_500)?
        .into_iter()
        .filter_map(|x| String::from(x).try_into().ok())
        .collect();

    let response = name::GetNamesResponse { names };
    Ok(axum::Json(response))
}

#[derive(Deserialize)]
pub struct PublishDatabaseParams {
    name_or_identity: Option<NameOrIdentity>,
}

#[derive(Deserialize)]
pub struct PublishDatabaseQueryParams {
    #[serde(default)]
    clear: bool,
    num_replicas: Option<usize>,
}

use std::env;
fn require_spacetime_auth_for_creation() -> bool {
    env::var("TEMP_REQUIRE_SPACETIME_AUTH").is_ok_and(|v| !v.is_empty())
}

// A hacky function to let us restrict database creation on maincloud.
fn allow_creation(auth: &SpacetimeAuth) -> Result<(), ErrorResponse> {
    if !require_spacetime_auth_for_creation() {
        return Ok(());
    }
    if auth.issuer.trim_end_matches('/') == "https://auth.spacetimedb.com" {
        Ok(())
    } else {
        log::trace!("Rejecting creation request because auth issuer is {}", auth.issuer);
        Err((
            StatusCode::UNAUTHORIZED,
            "To create a database, you must be logged in with a SpacetimeDB account.",
        )
            .into())
    }
}

pub async fn publish<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams { name_or_identity }): Path<PublishDatabaseParams>,
    Query(PublishDatabaseQueryParams { clear, num_replicas }): Query<PublishDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail.

    let (database_identity, db_name) = match &name_or_identity {
        Some(noa) => match noa.try_resolve(&ctx).await? {
            Ok(resolved) => (resolved, noa.name()),
            Err(name) => {
                // `name_or_identity` was a `NameOrIdentity::Name`, but no record
                // exists yet. Create it now with a fresh identity.
                allow_creation(&auth)?;
                let database_auth = SpacetimeAuth::alloc(&ctx).await?;
                let database_identity = database_auth.identity;
                let tld: name::Tld = name.clone().into();
                let tld = match ctx.register_tld(&auth.identity, tld).await.map_err(log_and_500)? {
                    name::RegisterTldResult::Success { domain }
                    | name::RegisterTldResult::AlreadyRegistered { domain } => domain,
                    name::RegisterTldResult::Unauthorized { .. } => {
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            axum::Json(PublishResult::PermissionDenied { name: name.clone() }),
                        )
                            .into())
                    }
                };
                let res = ctx
                    .create_dns_record(&auth.identity, &tld.into(), &database_identity)
                    .await
                    .map_err(log_and_500)?;
                match res {
                    name::InsertDomainResult::Success { .. } => {}
                    name::InsertDomainResult::TldNotRegistered { .. }
                    | name::InsertDomainResult::PermissionDenied { .. } => {
                        return Err(log_and_500("impossible: we just registered the tld"))
                    }
                    name::InsertDomainResult::OtherError(e) => return Err(log_and_500(e)),
                }
                (database_identity, Some(name))
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
        if !exists {
            allow_creation(&auth)?;
        }

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

    let num_replicas = num_replicas
        .map(|n| {
            let n = u8::try_from(n).map_err(|_| (StatusCode::BAD_REQUEST, "Replication factor {n} out of bounds"))?;
            Ok::<_, ErrorResponse>(NonZeroU8::new(n))
        })
        .transpose()?
        .flatten();

    let maybe_updated = ctx
        .publish_database(
            &auth.identity,
            DatabaseDef {
                database_identity,
                program_bytes: body.into(),
                num_replicas,
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
        domain: db_name.cloned(),
        database_identity,
        op,
    }))
}

#[derive(Deserialize)]
pub struct DeleteDatabaseParams {
    name_or_identity: NameOrIdentity,
}

pub async fn delete_database<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DeleteDatabaseParams { name_or_identity }): Path<DeleteDatabaseParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = name_or_identity.resolve(&ctx).await?;

    ctx.delete_database(&auth.identity, &database_identity)
        .await
        .map_err(log_and_500)?;

    Ok(())
}

#[derive(Deserialize)]
pub struct AddNameParams {
    name_or_identity: NameOrIdentity,
}

pub async fn add_name<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(AddNameParams { name_or_identity }): Path<AddNameParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    name: String,
) -> axum::response::Result<impl IntoResponse> {
    let name = DatabaseName::try_from(name).map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    let database_identity = name_or_identity.resolve(&ctx).await?;

    let response = ctx
        .create_dns_record(&auth.identity, &name.into(), &database_identity)
        .await
        // TODO: better error code handling
        .map_err(log_and_500)?;

    let code = match response {
        name::InsertDomainResult::Success { .. } => StatusCode::OK,
        name::InsertDomainResult::TldNotRegistered { .. } => StatusCode::BAD_REQUEST,
        name::InsertDomainResult::PermissionDenied { .. } => StatusCode::UNAUTHORIZED,
        name::InsertDomainResult::OtherError(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    Ok((code, axum::Json(response)))
}

#[derive(Deserialize)]
pub struct SetNamesParams {
    name_or_identity: NameOrIdentity,
}

pub async fn set_names<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(SetNamesParams { name_or_identity }): Path<SetNamesParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    names: axum::Json<Vec<String>>,
) -> axum::response::Result<impl IntoResponse> {
    let validated_names = names
        .0
        .into_iter()
        .map(|s| DatabaseName::from_str(&s).map(DomainName::from).map_err(|e| (s, e)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|(input, e)| (StatusCode::BAD_REQUEST, format!("Error parsing `{input}`: {e}")))?;

    let database_identity = name_or_identity.resolve(&ctx).await?;

    let database = ctx.get_database_by_identity(&database_identity).map_err(log_and_500)?;
    let Some(database) = database else {
        return Ok((
            StatusCode::NOT_FOUND,
            axum::Json(name::SetDomainsResult::DatabaseNotFound),
        ));
    };

    if database.owner_identity != auth.identity {
        return Ok((
            StatusCode::UNAUTHORIZED,
            axum::Json(name::SetDomainsResult::NotYourDatabase {
                database: database.database_identity,
            }),
        ));
    }

    let response = ctx
        .replace_dns_records(&database_identity, &database.owner_identity, &validated_names)
        .await
        .map_err(log_and_500)?;
    let status = match response {
        name::SetDomainsResult::Success => StatusCode::OK,
        name::SetDomainsResult::PermissionDenied { .. }
        | name::SetDomainsResult::PermissionDeniedOnAny { .. }
        | name::SetDomainsResult::NotYourDatabase { .. } => StatusCode::UNAUTHORIZED,
        name::SetDomainsResult::DatabaseNotFound => StatusCode::NOT_FOUND,
        name::SetDomainsResult::OtherError(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    Ok((status, axum::Json(response)))
}

/// This struct allows the edition to customize `/database` routes more meticulously.
pub struct DatabaseRoutes<S> {
    /// POST /database
    pub root_post: MethodRouter<S>,
    /// PUT: /database/:name_or_identity
    pub db_put: MethodRouter<S>,
    /// GET: /database/:name_or_identity
    pub db_get: MethodRouter<S>,
    /// DELETE: /database/:name_or_identity
    pub db_delete: MethodRouter<S>,
    /// GET: /database/:name_or_identity/names
    pub names_get: MethodRouter<S>,
    /// POST: /database/:name_or_identity/names
    pub names_post: MethodRouter<S>,
    /// PUT: /database/:name_or_identity/names
    pub names_put: MethodRouter<S>,
    /// GET: /database/:name_or_identity/identity
    pub identity_get: MethodRouter<S>,
    /// GET: /database/:name_or_identity/subscribe
    pub subscribe_get: MethodRouter<S>,
    /// POST: /database/:name_or_identity/call/:reducer
    pub call_reducer_post: MethodRouter<S>,
    /// GET: /database/:name_or_identity/schema
    pub schema_get: MethodRouter<S>,
    /// GET: /database/:name_or_identity/logs
    pub logs_get: MethodRouter<S>,
    /// POST: /database/:name_or_identity/sql
    pub sql_post: MethodRouter<S>,
}

impl<S> Default for DatabaseRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    fn default() -> Self {
        use axum::routing::{delete, get, post, put};
        Self {
            root_post: post(publish::<S>),
            db_put: put(publish::<S>),
            db_get: get(db_info::<S>),
            db_delete: delete(delete_database::<S>),
            names_get: get(get_names::<S>),
            names_post: post(add_name::<S>),
            names_put: put(set_names::<S>),
            identity_get: get(get_identity::<S>),
            subscribe_get: get(handle_websocket::<S>),
            call_reducer_post: post(call::<S>),
            schema_get: get(schema::<S>),
            logs_get: get(logs::<S>),
            sql_post: post(sql::<S>),
        }
    }
}

impl<S> DatabaseRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    pub fn into_router(self, ctx: S) -> axum::Router<S> {
        let db_router = axum::Router::<S>::new()
            .route("/", self.db_put)
            .route("/", self.db_get)
            .route("/", self.db_delete)
            .route("/names", self.names_get)
            .route("/names", self.names_post)
            .route("/names", self.names_put)
            .route("/identity", self.identity_get)
            .route("/subscribe", self.subscribe_get)
            .route("/call/:reducer", self.call_reducer_post)
            .route("/schema", self.schema_get)
            .route("/logs", self.logs_get)
            .route("/sql", self.sql_post);

        axum::Router::new()
            .route("/", self.root_post)
            .nest("/:name_or_identity", db_router)
            .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
    }
}
