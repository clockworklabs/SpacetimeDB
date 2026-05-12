use std::borrow::Cow;
use std::num::NonZeroU8;
use std::str::FromStr;
use std::time::Duration;
use std::{env, io};

use crate::auth::{
    anon_auth_middleware, SpacetimeAuth, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros, SpacetimeIdentity,
    SpacetimeIdentityToken,
};
use crate::routes::subscribe::generate_random_connection_id;
use crate::util::serde::humantime_duration;
pub use crate::util::{ByteStringBody, NameOrIdentity};
use crate::{
    log_and_500, Action, Authorization, ControlStateDelegate, DatabaseDef, DatabaseResetDef, Host, MaybeMisdirected,
    NodeDelegate, Unauthorized,
};
use axum::body::{Body, Bytes};
use axum::extract::{OriginalUri, Path, Query, Request, State};
use axum::response::{ErrorResponse, IntoResponse};
use axum::routing::MethodRouter;
use axum::Extension;
use axum_extra::TypedHeader;
use derive_more::From;
use futures::TryStreamExt;
use http::StatusCode;
use http_body_util::BodyExt;
use log::{info, warn};
use serde::Deserialize;
use spacetimedb::auth::identity::ConnectionAuthCtx;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::module_host::{ClientConnectedError, DurabilityExited};
use spacetimedb::host::{CallResult, UpdateDatabaseResult};
use spacetimedb::host::{FunctionArgs, MigratePlanResult};
use spacetimedb::host::{ModuleHost, ReducerOutcome};
use spacetimedb::host::{ProcedureCallError, ReducerCallError};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, HostType};
use spacetimedb_client_api_messages::http::SqlStmtResult;
use spacetimedb_client_api_messages::name::{
    self, DatabaseName, DomainName, MigrationPolicy, PrePublishAutoMigrateResult, PrePublishManualMigrateResult,
    PrePublishResult, PrettyPrintStyle, PublishOp, PublishResult,
};
use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::http as st_http;
use spacetimedb_lib::{sats, AlgebraicValue, Hash, ProductValue, Timestamp};
use spacetimedb_schema::auto_migrate::{
    MigrationPolicy as SchemaMigrationPolicy, MigrationToken, PrettyPrintStyle as AutoMigratePrettyPrintStyle,
};
use tokio::sync::oneshot;
use tokio::time::error::Elapsed;
use tokio::time::timeout;

use super::subscribe::{handle_websocket, HasWebSocketOptions};

fn require_spacetime_auth_for_creation() -> Option<String> {
    // If the string is a non-empty value, return the string to be used as the required issuer
    // TODO(cloutiertyler): This env var replaces TEMP_REQUIRE_SPACETIME_AUTH,
    // we should remove that one in the future. We may eventually remove
    // the below restriction entirely as well in Maincloud.
    match env::var("TEMP_SPACETIMEAUTH_ISSUER_REQUIRED_TO_PUBLISH") {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

// A hacky function to let us restrict database creation on maincloud.
fn allow_creation(auth: &SpacetimeAuth) -> Result<(), ErrorResponse> {
    let Some(required_issuer) = require_spacetime_auth_for_creation() else {
        return Ok(());
    };
    let issuer = auth.claims.issuer.trim_end_matches('/');
    if issuer == required_issuer {
        Ok(())
    } else {
        log::trace!(
            "Rejecting creation request because auth issuer is {} and required issuer is {}",
            auth.claims.issuer,
            required_issuer
        );
        Err((
            StatusCode::UNAUTHORIZED,
            "To create a database, you must be logged in with a SpacetimeDB account.",
        )
            .into())
    }
}
#[derive(Deserialize)]
pub struct CallParams {
    name_or_identity: NameOrIdentity,
    reducer: String,
}

pub const NO_SUCH_DATABASE: (StatusCode, &str) = (StatusCode::NOT_FOUND, "No such database.");
const MISDIRECTED: (StatusCode, &str) = (StatusCode::NOT_FOUND, "Database is not scheduled on this host");

fn map_reducer_error(e: ReducerCallError, reducer: &str) -> (StatusCode, String) {
    let status_code = match e {
        ReducerCallError::Args(_) => {
            log::debug!("Attempt to call reducer {reducer} with invalid arguments");
            StatusCode::BAD_REQUEST
        }
        ReducerCallError::NoSuchModule(_) | ReducerCallError::ScheduleReducerNotFound => StatusCode::NOT_FOUND,
        ReducerCallError::NoSuchReducer => {
            log::debug!("Attempt to call non-existent reducer {reducer}");
            StatusCode::NOT_FOUND
        }
        ReducerCallError::WorkerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        ReducerCallError::LifecycleReducer(lifecycle) => {
            log::debug!("Attempt to call {lifecycle:?} lifecycle reducer {reducer}");
            StatusCode::BAD_REQUEST
        }
    };

    log::debug!("Error while invoking reducer {e:#}");
    (status_code, format!("{:#}", anyhow::anyhow!(e)))
}

fn map_procedure_error(e: ProcedureCallError, procedure: &str) -> (StatusCode, String) {
    let status_code = match e {
        ProcedureCallError::Args(_) => {
            log::debug!("Attempt to call procedure {procedure} with invalid arguments");
            StatusCode::BAD_REQUEST
        }
        ProcedureCallError::NoSuchModule(_) => StatusCode::NOT_FOUND,
        ProcedureCallError::NoSuchProcedure => {
            log::debug!("Attempt to call non-existent procedure OR reducer {procedure}");
            StatusCode::NOT_FOUND
        }
        ProcedureCallError::OutOfEnergy => StatusCode::PAYMENT_REQUIRED,
        ProcedureCallError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    log::error!("Error while invoking procedure {e:#}");
    (status_code, format!("{:#}", anyhow::anyhow!(e)))
}

/// Call a reducer or procedure on the specified database module.
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
    assert_content_type_json(content_type)?;

    let caller_identity = auth.claims.identity;

    let args = FunctionArgs::Json(body);

    // HTTP callers always need a connection ID to provide to connect/disconnect,
    // so generate one.
    let connection_id = generate_random_connection_id();

    let (module, Database { owner_identity, .. }) = find_module_and_database(&worker_ctx, name_or_identity).await?;

    // Call the database's `client_connected` reducer, if any.
    // If it fails or rejects the connection, bail.
    module
        .call_identity_connected(auth.into(), connection_id)
        .await
        .map_err(client_connected_error_to_response)?;

    let result = match module
        .call_reducer(
            caller_identity,
            Some(connection_id),
            None,
            None,
            None,
            &reducer,
            args.clone(),
        )
        .await
    {
        Ok(rcr) => Ok(CallResult::Reducer(rcr)),
        Err(ReducerCallError::NoSuchReducer | ReducerCallError::ScheduleReducerNotFound) => {
            // Not a reducer — try procedure instead
            match module
                .call_procedure(caller_identity, Some(connection_id), None, &reducer, args)
                .await
                .result
            {
                Ok(res) => Ok(CallResult::Procedure(res)),
                Err(e) => Err(map_procedure_error(e, &reducer)),
            }
        }
        Err(e) => Err(map_reducer_error(e, &reducer)),
    };

    module
        .call_identity_disconnected(caller_identity, connection_id)
        .await
        .map_err(client_disconnected_error_to_response)?;

    match result {
        Ok(CallResult::Reducer(result)) => {
            let (status, body) = reducer_outcome_response(&owner_identity, &reducer, result.outcome);
            Ok((
                status,
                TypedHeader(SpacetimeEnergyUsed(result.energy_used)),
                TypedHeader(SpacetimeExecutionDurationMicros(result.execution_duration)),
                body,
            )
                .into_response())
        }
        Ok(CallResult::Procedure(result)) => {
            // Procedures don't assign a special meaning to error returns, unlike reducers,
            // as there's no transaction for them to automatically abort.
            // Instead, we just pass on their return value with the OK status so long as we successfully invoked the procedure.
            let (status, body) = procedure_outcome_response(result.return_val);
            Ok((
                status,
                TypedHeader(SpacetimeExecutionDurationMicros(result.execution_duration)),
                body,
            )
                .into_response())
        }
        Err(e) => Err((e.0, e.1).into()),
    }
}

#[derive(Deserialize)]
pub struct HttpRouteRootParams {
    name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct HttpRouteParams {
    name_or_identity: NameOrIdentity,
    path: String,
}

pub async fn handle_http_route_root<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    Path(HttpRouteRootParams { name_or_identity }): Path<HttpRouteRootParams>,
    OriginalUri(original_uri): OriginalUri,
    request: Request,
) -> axum::response::Result<impl IntoResponse> {
    handle_http_route_impl(worker_ctx, name_or_identity, "".to_string(), original_uri, request).await
}

pub async fn handle_http_route_root_slash<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    Path(HttpRouteRootParams { name_or_identity }): Path<HttpRouteRootParams>,
    OriginalUri(original_uri): OriginalUri,
    request: Request,
) -> axum::response::Result<impl IntoResponse> {
    handle_http_route_impl(worker_ctx, name_or_identity, "/".to_string(), original_uri, request).await
}

pub async fn handle_http_route<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    Path(HttpRouteParams { name_or_identity, path }): Path<HttpRouteParams>,
    OriginalUri(original_uri): OriginalUri,
    request: Request,
) -> axum::response::Result<impl IntoResponse> {
    handle_http_route_impl(worker_ctx, name_or_identity, format!("/{path}"), original_uri, request).await
}

/// Error response body for unknown user-defined HTTP route.
const NO_SUCH_ROUTE: &str = "Database has not registered a handler for this route";

async fn handle_http_route_impl<S: ControlStateDelegate + NodeDelegate>(
    worker_ctx: S,
    name_or_identity: NameOrIdentity,
    handler_path: String,
    original_uri: http::Uri,
    request: Request,
) -> axum::response::Result<impl IntoResponse> {
    let (parts, body) = request.into_parts();
    let st_method = http_method_to_st(&parts.method);

    let (module, _database) = find_module_and_database(&worker_ctx, name_or_identity).await?;
    let module_def = &module.info().module_def;

    let Some((handler_id, _handler_def, _route_def)) = module_def.match_http_route(&st_method, &handler_path) else {
        return Ok((StatusCode::NOT_FOUND, NO_SUCH_ROUTE).into_response());
    };

    // TODO(streaming-http): stop collecting the full request body here once route dispatch can
    // hand Axum's body stream through the WASM handler ABI incrementally.
    let body = body.collect().await.map_err(log_and_500)?.to_bytes();
    let forwarded_uri = reconstruct_external_uri(&original_uri, &parts.headers);
    let request = st_http::Request {
        method: st_method.clone(),
        headers: headers_to_st(parts.headers),
        timeout: None,
        uri: forwarded_uri,
        version: http_version_to_st(parts.version),
    };

    let response = match module.call_http_handler(handler_id, request, body).await {
        Ok(response) => response,
        Err(spacetimedb::host::module_host::HttpHandlerCallError::NoSuchHandler) => {
            return Ok((StatusCode::NOT_FOUND, NO_SUCH_ROUTE).into_response());
        }
        Err(spacetimedb::host::module_host::HttpHandlerCallError::NoSuchModule(_)) => {
            return Err(NO_SUCH_DATABASE.into());
        }
        Err(spacetimedb::host::module_host::HttpHandlerCallError::InternalError(err)) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, err).into());
        }
    };

    let response = response_from_st(response.0, response.1)?;
    Ok(response.into_response())
}

/// Return the URI that would have been in the original request, including scheme, domain and full path.
///
/// This is necessary because Axum strips the URI as it processes routing,
/// causing the request seen by the handler function to contain only the suffix that participated in routing
/// for the last service involved.
///
/// We want to show the entire URI to the user-defined handler, so we reconstruct it based on X-Forwarded headers.
fn reconstruct_external_uri(original_uri: &http::Uri, headers: &http::HeaderMap) -> String {
    if original_uri.scheme().is_some() && original_uri.authority().is_some() {
        return original_uri.to_string();
    }

    let scheme = forwarded_header(headers, "x-forwarded-proto")
        .or_else(|| original_uri.scheme_str().map(str::to_owned))
        .unwrap_or_else(|| "http".to_string());
    let authority = forwarded_header(headers, "x-forwarded-host")
        .or_else(|| {
            headers
                .get(http::header::HOST)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .or_else(|| original_uri.authority().map(|authority| authority.to_string()));
    let path_and_query = original_uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or_else(|| original_uri.path());

    if let Some(authority) = authority {
        format!("{scheme}://{authority}{path_and_query}")
    } else {
        original_uri.to_string()
    }
}

fn forwarded_header(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn assert_content_type_json(content_type: headers::ContentType) -> axum::response::Result<()> {
    if content_type != headers::ContentType::json() {
        Err(axum::extract::rejection::MissingJsonContentType::default().into())
    } else {
        Ok(())
    }
}

fn http_method_to_st(method: &http::Method) -> st_http::Method {
    match *method {
        http::Method::GET => st_http::Method::Get,
        http::Method::HEAD => st_http::Method::Head,
        http::Method::POST => st_http::Method::Post,
        http::Method::PUT => st_http::Method::Put,
        http::Method::DELETE => st_http::Method::Delete,
        http::Method::CONNECT => st_http::Method::Connect,
        http::Method::OPTIONS => st_http::Method::Options,
        http::Method::TRACE => st_http::Method::Trace,
        http::Method::PATCH => st_http::Method::Patch,
        _ => st_http::Method::Extension(method.to_string()),
    }
}

fn http_version_to_st(version: http::Version) -> st_http::Version {
    match version {
        http::Version::HTTP_09 => st_http::Version::Http09,
        http::Version::HTTP_10 => st_http::Version::Http10,
        http::Version::HTTP_11 => st_http::Version::Http11,
        http::Version::HTTP_2 => st_http::Version::Http2,
        http::Version::HTTP_3 => st_http::Version::Http3,
        _ => unreachable!("unknown HTTP version: {version:?}"),
    }
}

fn headers_to_st(headers: http::HeaderMap) -> st_http::Headers {
    headers
        .into_iter()
        .map(|(k, v)| (k.map(|k| k.as_str().into()), v.as_bytes().into()))
        .collect()
}

fn response_from_st(response: st_http::Response, body: Bytes) -> axum::response::Result<http::Response<Body>> {
    let st_http::Response { headers, version, code } = response;

    // TODO(streaming-http): stop materializing the whole response body before building the Axum
    // response once the handler ABI can stream directly into the outbound HTTP body.
    let mut response = http::Response::new(Body::from(body));
    *response.version_mut() = match version {
        st_http::Version::Http09 => http::Version::HTTP_09,
        st_http::Version::Http10 => http::Version::HTTP_10,
        st_http::Version::Http11 => http::Version::HTTP_11,
        st_http::Version::Http2 => http::Version::HTTP_2,
        st_http::Version::Http3 => http::Version::HTTP_3,
    };
    *response.status_mut() = http::StatusCode::from_u16(code).map_err(log_and_500)?;
    for (name, value) in headers.into_iter() {
        let name = http::HeaderName::from_bytes(name.as_bytes()).map_err(log_and_500)?;
        let value = http::HeaderValue::from_bytes(&value).map_err(log_and_500)?;
        response.headers_mut().append(name, value);
    }

    Ok(response)
}

fn reducer_outcome_response(
    owner_identity: &Identity,
    reducer: &str,
    outcome: ReducerOutcome,
) -> (StatusCode, Box<str>) {
    match outcome {
        ReducerOutcome::Committed => (StatusCode::OK, "".into()),
        ReducerOutcome::Failed(errmsg) => {
            // TODO: different status code? this is what cloudflare uses, sorta
            (StatusCode::from_u16(530).unwrap(), *errmsg)
        }
        ReducerOutcome::BudgetExceeded => {
            log::warn!("Node's energy budget exceeded for identity: {owner_identity} while executing {reducer}");
            (StatusCode::PAYMENT_REQUIRED, "Module energy budget exhausted.".into())
        }
    }
}

fn client_connected_error_to_response(err: ClientConnectedError) -> ErrorResponse {
    match err {
        // If `call_identity_connected` returns `Err(Rejected)`, then the `client_connected` reducer errored,
        // meaning the connection was refused. Return 403 forbidden.
        ClientConnectedError::Rejected(msg) => (StatusCode::FORBIDDEN, msg).into(),
        // If `call_identity_connected` returns `Err(OutOfEnergy)`,
        // then, well, the database is out of energy.
        // Return 503 service unavailable.
        ClientConnectedError::OutOfEnergy => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()).into(),
        // If `call_identity_connected` returns `Err(ReducerCall)`,
        // something went wrong while invoking the `client_connected` reducer.
        // I (pgoldman 2025-03-27) am not really sure how this would happen,
        // but we returned 404 not found in this case prior to my editing this code,
        // so I guess let's keep doing that.
        ClientConnectedError::ReducerCall(e) => (StatusCode::NOT_FOUND, format!("{:#}", anyhow::anyhow!(e))).into(),
        // If `call_identity_connected` returns `Err(DBError)`,
        // then the module didn't define `client_connected`,
        // but something went wrong when we tried to insert into `st_client`.
        // That's weird and scary, so return 500 internal error.
        ClientConnectedError::DBError(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into(),
    }
}

/// If `call_identity_disconnected` errors, something is very wrong:
/// it means we tried to delete the `st_client` row but failed.
///
/// Note that `call_identity_disconnected` swallows errors from the `client_disconnected` reducer.
/// Slap a 500 on it and pray.
fn client_disconnected_error_to_response(err: ReducerCallError) -> ErrorResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{:#}", anyhow::anyhow!(err))).into()
}

async fn find_leader_and_database<S: ControlStateDelegate + NodeDelegate>(
    worker_ctx: &S,
    name_or_identity: NameOrIdentity,
) -> axum::response::Result<(Host, Database)> {
    let db_identity = name_or_identity.resolve(worker_ctx).await?;
    let database = worker_ctx_find_database(worker_ctx, &db_identity)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", db_identity.to_hex());
            NO_SUCH_DATABASE
        })?;

    let leader = worker_ctx.leader(database.id).await.map_err(Into::into)?;

    Ok((leader, database))
}

async fn find_module_and_database<S: ControlStateDelegate + NodeDelegate>(
    worker_ctx: &S,
    name_or_identity: NameOrIdentity,
) -> axum::response::Result<(ModuleHost, Database)> {
    let (leader, database) = find_leader_and_database(worker_ctx, name_or_identity).await?;
    let module = leader.module().await.map_err(log_and_500)?;

    Ok((module, database))
}

#[derive(Debug, derive_more::From)]
pub enum DBCallErr {
    HandlerError(ErrorResponse),
    NoSuchDatabase,
    InstanceNotScheduled,
}

fn procedure_outcome_response(return_val: AlgebraicValue) -> (StatusCode, axum::response::Response) {
    (
        StatusCode::OK,
        axum::Json(sats::serde::SerdeWrapper(return_val)).into_response(),
    )
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
    #[serde(rename = "10")]
    V10,
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
    let (leader, _) = find_leader_and_database(&worker_ctx, name_or_identity).await?;
    // Wait for the module to finish loading rather than returning an immediate
    // 500 error. The database may still be initializing (replaying the log,
    // running init reducers, etc.).
    let module = leader
        .wait_for_module(std::time::Duration::from_secs(10))
        .await
        .map_err(log_and_500)?;

    let module_def = &module.info.module_def;
    let response_json = match version {
        SchemaVersion::V9 => {
            let raw = RawModuleDefV9::from(module_def.as_ref().clone());
            axum::Json(sats::serde::SerdeWrapper(raw)).into_response()
        }
        SchemaVersion::V10 => {
            let raw = RawModuleDefV10::from(module_def.as_ref().clone());
            axum::Json(sats::serde::SerdeWrapper(raw)).into_response()
        }
    };

    Ok((
        TypedHeader(SpacetimeIdentity(auth.claims.identity)),
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
    log::trace!("Trying to resolve database identity: {name_or_identity:?}");
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
    S: ControlStateDelegate + NodeDelegate + Authorization,
{
    // You should not be able to read the logs from a database that you do not own
    // so, unless you are the owner, this will fail.

    let database_identity: Identity = name_or_identity.resolve(&worker_ctx).await?;
    let database = worker_ctx_find_database(&worker_ctx, &database_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;

    worker_ctx
        .authorize_action(auth.claims.identity, database.database_identity, Action::ViewModuleLogs)
        .await?;

    fn log_err(database: Identity) -> impl Fn(&io::Error) {
        move |e| warn!("error serving module logs for database {database}: {e:#}")
    }

    let body = match worker_ctx.leader(database.id).await {
        Ok(host) => {
            let module = host.module().await.map_err(log_and_500)?;
            let logs = module.database_logger().tail(num_lines, follow).await.map_err(|e| {
                warn!("database={database_identity} unable to tail logs: {e:#}");
                (StatusCode::SERVICE_UNAVAILABLE, "Logs are temporarily not available")
            })?;
            Body::from_stream(logs.inspect_err(log_err(database_identity)))
        }
        Err(e) if e.is_misdirected() => return Err(MISDIRECTED.into()),
        // If this is the right node for the current or last-known leader,
        // we may still be able to serve logs from disk,
        // even if we can't get hold of a running [ModuleHost].
        Err(e) => {
            warn!("could not obtain leader host for module logs: {e:#}");
            let Some(replica) = worker_ctx.get_leader_replica_by_database(database.id).await else {
                return Err(MISDIRECTED.into());
            };
            let logs_dir = worker_ctx.module_logs_dir(replica.id);
            if !logs_dir.0.try_exists().map_err(log_and_500)? {
                // Probably an in-memory database.
                // Logs may become available at a later time.
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Database is not running and doesn't have persistent logs",
                )
                    .into());
            }
            let logs = DatabaseLogger::read_latest_on_disk(logs_dir, num_lines);
            Body::from_stream(logs.inspect_err(log_err(database_identity)))
        }
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

pub(crate) async fn worker_ctx_find_database(
    worker_ctx: &(impl ControlStateDelegate + ?Sized),
    database_identity: &Identity,
) -> axum::response::Result<Option<Database>> {
    worker_ctx
        .get_database_by_identity(database_identity)
        .await
        .map_err(log_and_500)
}

#[derive(Deserialize)]
pub struct SqlParams {
    pub name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct SqlQueryParams {
    /// If `true`, return the query result only after its transaction offset
    /// is confirmed to be durable.
    #[serde(default)]
    pub confirmed: Option<bool>,
}

pub async fn sql_direct<S>(
    worker_ctx: S,
    SqlParams { name_or_identity }: SqlParams,
    SqlQueryParams { confirmed }: SqlQueryParams,
    caller_identity: Identity,
    caller_auth: ConnectionAuthCtx,
    sql: String,
) -> axum::response::Result<Vec<SqlStmtResult<ProductValue>>>
where
    S: NodeDelegate + ControlStateDelegate + Authorization,
{
    let connection_id = generate_random_connection_id();

    let (host, database) = find_leader_and_database(&worker_ctx, name_or_identity).await?;

    // Run the module's client_connected reducer, if any.
    // If it rejects the connection, bail before executing SQL.
    let module = host.module().await.map_err(log_and_500)?;
    module
        .call_identity_connected(caller_auth, connection_id)
        .await
        .map_err(client_connected_error_to_response)?;

    let result = async {
        let sql_auth = worker_ctx
            .authorize_sql(caller_identity, database.database_identity)
            .await?;

        host.exec_sql(
            sql_auth,
            database,
            confirmed.unwrap_or(crate::DEFAULT_CONFIRMED_READS),
            sql,
        )
        .await
    }
    .await;

    // Always disconnect, even if authorization or execution failed.
    module
        .call_identity_disconnected(caller_identity, connection_id)
        .await
        .map_err(client_disconnected_error_to_response)?;

    result
}

pub async fn sql<S>(
    State(worker_ctx): State<S>,
    Path(name_or_identity): Path<SqlParams>,
    Query(params): Query<SqlQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: String,
) -> axum::response::Result<impl IntoResponse>
where
    S: NodeDelegate + ControlStateDelegate + Authorization,
{
    let caller_identity = auth.claims.identity;
    let caller_auth: ConnectionAuthCtx = auth.into();
    let json = sql_direct(worker_ctx, name_or_identity, params, caller_identity, caller_auth, body).await?;

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
        .await
        .map_err(log_and_500)?
        .into_iter()
        .filter_map(|x| String::from(x).try_into().ok())
        .collect();

    let response = name::GetNamesResponse { names };
    Ok(axum::Json(response))
}

#[derive(Deserialize)]
pub struct ResetDatabaseParams {
    name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct ResetDatabaseQueryParams {
    num_replicas: Option<usize>,
    #[serde(default)]
    host_type: HostType,
}

pub async fn reset<S: NodeDelegate + ControlStateDelegate + Authorization>(
    State(ctx): State<S>,
    Path(ResetDatabaseParams { name_or_identity }): Path<ResetDatabaseParams>,
    Query(ResetDatabaseQueryParams {
        num_replicas,
        host_type,
    }): Query<ResetDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    program_bytes: Option<Bytes>,
) -> axum::response::Result<axum::Json<PublishResult>> {
    let database_identity = name_or_identity.resolve(&ctx).await?;
    let database = worker_ctx_find_database(&ctx, &database_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;

    ctx.authorize_action(auth.claims.identity, database.database_identity, Action::ResetDatabase)
        .await?;

    let num_replicas = num_replicas.map(validate_replication_factor).transpose()?.flatten();
    ctx.reset_database(
        &auth.claims.identity,
        DatabaseResetDef {
            database_identity,
            program_bytes,
            num_replicas,
            host_type: Some(host_type),
        },
    )
    .await
    .map_err(log_and_500)?;

    Ok(axum::Json(PublishResult::Success {
        domain: name_or_identity.name().cloned(),
        database_identity,
        op: PublishOp::Updated,
    }))
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
    /// [`Hash`] of [`MigrationToken`]` to be checked if `MigrationPolicy::BreakClients` is set.
    ///
    /// Users obtain such a hash via the `/database/:name_or_identity/pre-publish POST` route.
    /// This is a safeguard to require explicit approval for updates which will break clients.
    token: Option<Hash>,
    #[serde(default)]
    policy: MigrationPolicy,
    #[serde(default)]
    host_type: HostType,
    parent: Option<NameOrIdentity>,
    #[serde(alias = "org")]
    organization: Option<NameOrIdentity>,
    /// Duration to wait for a database update to become confirmed (i.e. durable).
    ///
    /// The value is parsed via the `humantime` crate, e.g. "1m", "23s", "5min".
    ///
    /// If not given, defaults to [default_update_confirmation_timeout].
    /// The maximum timeout is capped by [MAX_UPDATE_CONFIRMATION_TIMEOUT].
    ///
    /// The parameter has no effect when creating a new database.
    #[serde(with = "humantime_duration", default = "default_update_confirmation_timeout")]
    update_confirmation_timeout: Duration,
}

/// Default timeout for a database update to become confirmed / durable.
///
/// Currently, the value is 5s.
const fn default_update_confirmation_timeout() -> Duration {
    Duration::from_secs(5)
}

/// Maximum timeout for a database update to become confirmed / durable.
///
/// If a replication group doesn't converge within this time span, it is
/// probably not making progress at all.
///
/// Currently, the value is 5min.
const MAX_UPDATE_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub async fn publish<S: NodeDelegate + ControlStateDelegate + Authorization>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams { name_or_identity }): Path<PublishDatabaseParams>,
    Query(PublishDatabaseQueryParams {
        clear,
        num_replicas,
        token,
        policy,
        host_type,
        parent,
        organization,
        update_confirmation_timeout: confirmation_timeout,
    }): Query<PublishDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    program_bytes: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    // If `clear`, check that the database exists and delegate to `reset`.
    // If it doesn't exist, ignore the `clear` parameter.
    // TODO: Replace with actual redirect at the next possible version bump.
    if clear {
        let name_or_identity = name_or_identity
            .as_ref()
            .ok_or_else(|| bad_request("Clear database requires database name or identity".into()))?;
        let database_identity = name_or_identity.try_resolve(&ctx).await.map_err(log_and_500)?;
        if let Ok(identity) = database_identity {
            let exists = ctx
                .get_database_by_identity(&identity)
                .await
                .map_err(log_and_500)?
                .is_some();
            if exists {
                if parent.is_some() {
                    return Err(bad_request(
                        "Setting the parent of an existing database is not supported".into(),
                    ));
                }

                return self::reset(
                    State(ctx),
                    Path(ResetDatabaseParams {
                        name_or_identity: name_or_identity.clone(),
                    }),
                    Query(ResetDatabaseQueryParams {
                        num_replicas,
                        host_type,
                    }),
                    Extension(auth),
                    Some(program_bytes),
                )
                .await;
            }
        }
    }

    let (database_identity, db_name) = get_or_create_identity_and_name(&ctx, &auth, name_or_identity.as_ref()).await?;
    let maybe_parent_database_identity = match parent.as_ref() {
        None => None,
        Some(parent) => parent.resolve(&ctx).await.map(Some)?,
    };
    let maybe_org_identity = match organization.as_ref() {
        None => None,
        Some(org) => org.resolve_namespace_owner(&ctx).await.map(Some)?,
    };

    // Check that the replication factor looks somewhat sane.
    let num_replicas = num_replicas.map(validate_replication_factor).transpose()?.flatten();

    log::trace!("Publishing to the identity: {}", database_identity.to_hex());

    // Check if the database already exists.
    let existing = ctx
        .get_database_by_identity(&database_identity)
        .await
        .map_err(log_and_500)?;
    match existing.as_ref() {
        None => {
            allow_creation(&auth)?;
            ctx.authorize_action(
                auth.claims.identity,
                database_identity,
                Action::CreateDatabase {
                    parent: maybe_parent_database_identity,
                    organization: maybe_org_identity,
                },
            )
            .await?;
        }
        Some(database) => {
            ctx.authorize_action(auth.claims.identity, database.database_identity, Action::UpdateDatabase)
                .await?;
        }
    }

    // Indicate in the response whether we created or updated the database.
    let publish_op = if existing.is_some() {
        PublishOp::Updated
    } else {
        PublishOp::Created
    };
    // If a parent is given, resolve to an existing database.
    let parent = if let Some(name_or_identity) = parent {
        let identity = name_or_identity
            .resolve(&ctx)
            .await
            .map_err(|_| bad_request(format!("Parent database {name_or_identity} not found").into()))?;
        Some(identity)
    } else {
        None
    };

    let schema_migration_policy = schema_migration_policy(policy, token)?;
    let maybe_updated = ctx
        .publish_database(
            &auth.claims.identity,
            DatabaseDef {
                database_identity,
                program_bytes,
                num_replicas,
                host_type,
                parent,
                organization: maybe_org_identity,
            },
            schema_migration_policy,
        )
        .await
        .map_err(log_and_500)?;

    let success = || {
        axum::Json(PublishResult::Success {
            domain: db_name.cloned(),
            database_identity,
            op: publish_op,
        })
    };
    match maybe_updated {
        Some(UpdateDatabaseResult::AutoMigrateError(errs)) => {
            Err(bad_request(format!("Database update rejected: {errs}").into()))
        }
        Some(UpdateDatabaseResult::ErrorExecutingMigration(err)) => Err(bad_request(
            format!("Failed to create or update the database: {err}").into(),
        )),
        None | Some(UpdateDatabaseResult::NoUpdateNeeded) => Ok(success()),
        Some(
            UpdateDatabaseResult::UpdatePerformed {
                tx_offset,
                durable_offset,
            }
            | UpdateDatabaseResult::UpdatePerformedWithClientDisconnect {
                tx_offset,
                durable_offset,
            },
        ) => {
            timeout(confirmation_timeout.min(MAX_UPDATE_CONFIRMATION_TIMEOUT), async {
                let tx_offset = tx_offset.await?;
                if let Some(mut durable_offset) = durable_offset {
                    durable_offset.wait_for(tx_offset).await?;
                }

                Ok::<_, UpdateConfirmationError>(())
            })
            .await
            .map_err(Into::into)
            .flatten()?;

            Ok(success())
        }
    }
}

#[derive(From)]
enum UpdateConfirmationError {
    Cancelled(oneshot::error::RecvError),
    Crashed(DurabilityExited),
    Timeout(Elapsed),
}

impl From<UpdateConfirmationError> for ErrorResponse {
    fn from(e: UpdateConfirmationError) -> Self {
        match e {
            UpdateConfirmationError::Cancelled(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Database update failed: transaction was cancelled",
            ),
            UpdateConfirmationError::Crashed(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Database update failed: database crashed while waiting for transaction confirmation",
            ),
            UpdateConfirmationError::Timeout(_) => (
                StatusCode::GATEWAY_TIMEOUT,
                "Database update failed: timeout waiting for transaction confirmation",
            ),
        }
        .into()
    }
}

/// Try to resolve `name_or_identity` to an [Identity] and [DatabaseName].
///
/// - If the database exists and has a name registered for it, return that.
/// - If the database does not exist, but `name_or_identity` is a name,
///   try to register the name and return alongside a newly allocated [Identity]
/// - Otherwise, if the database does not exist and `name_or_identity` is `None`,
///   allocate a fresh [Identity] and no name.
///
async fn get_or_create_identity_and_name<'a>(
    ctx: &(impl ControlStateDelegate + NodeDelegate),
    auth: &SpacetimeAuth,
    name_or_identity: Option<&'a NameOrIdentity>,
) -> axum::response::Result<(Identity, Option<&'a DatabaseName>)> {
    match name_or_identity {
        Some(noi) => match noi.try_resolve(ctx).await.map_err(log_and_500)? {
            Ok(resolved) => Ok((resolved, noi.name())),
            Err(name) => {
                // `name_or_identity` was a `NameOrIdentity::Name`, but no record
                // exists yet. Create it now with a fresh identity.
                allow_creation(auth)?;
                let database_auth = SpacetimeAuth::alloc(ctx).await?;
                let database_identity = database_auth.claims.identity;
                create_name(ctx, auth, &database_identity, name).await?;
                Ok((database_identity, Some(name)))
            }
        },
        None => {
            let database_auth = SpacetimeAuth::alloc(ctx).await?;
            let database_identity = database_auth.claims.identity;
            Ok((database_identity, None))
        }
    }
}

/// Try to register `name` for database `database_identity`.
async fn create_name(
    ctx: &(impl NodeDelegate + ControlStateDelegate),
    auth: &SpacetimeAuth,
    database_identity: &Identity,
    name: &DatabaseName,
) -> axum::response::Result<()> {
    let tld: name::Tld = name.clone().into();
    let tld = match ctx
        .register_tld(&auth.claims.identity, tld)
        .await
        .map_err(log_and_500)?
    {
        name::RegisterTldResult::Success { domain } | name::RegisterTldResult::AlreadyRegistered { domain } => domain,
        name::RegisterTldResult::Unauthorized { .. } => {
            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(PublishResult::PermissionDenied { name: name.clone() }),
            )
                .into())
        }
    };
    let res = ctx
        .create_dns_record(&auth.claims.identity, &tld.into(), database_identity)
        .await
        .map_err(log_and_500)?;
    match res {
        name::InsertDomainResult::Success { .. } => Ok(()),
        name::InsertDomainResult::TldNotRegistered { .. } | name::InsertDomainResult::PermissionDenied { .. } => {
            Err(log_and_500("impossible: we just registered the tld"))
        }
        name::InsertDomainResult::OtherError(e) => Err(log_and_500(e)),
    }
}

fn schema_migration_policy(
    policy: MigrationPolicy,
    token: Option<Hash>,
) -> axum::response::Result<SchemaMigrationPolicy> {
    const MISSING_TOKEN: &str = "Migration policy is set to `BreakClients`, but no migration token was provided.";

    match policy {
        MigrationPolicy::BreakClients => token
            .map(SchemaMigrationPolicy::BreakClients)
            .ok_or_else(|| bad_request(MISSING_TOKEN.into())),
        MigrationPolicy::Compatible => Ok(SchemaMigrationPolicy::Compatible),
    }
}

fn validate_replication_factor(n: usize) -> Result<Option<NonZeroU8>, ErrorResponse> {
    let n = u8::try_from(n).map_err(|_| bad_request(format!("Replication factor {n} out of bounds").into()))?;
    Ok(NonZeroU8::new(n))
}

fn bad_request(message: Cow<'static, str>) -> ErrorResponse {
    (StatusCode::BAD_REQUEST, message).into()
}

#[derive(serde::Deserialize)]
pub struct PrePublishParams {
    name_or_identity: NameOrIdentity,
}

#[derive(serde::Deserialize)]
pub struct PrePublishQueryParams {
    #[serde(default)]
    style: PrettyPrintStyle,
    #[serde(default)]
    host_type: HostType,
}

pub async fn pre_publish<S: NodeDelegate + ControlStateDelegate + Authorization>(
    State(ctx): State<S>,
    Path(PrePublishParams { name_or_identity }): Path<PrePublishParams>,
    Query(PrePublishQueryParams { style, host_type }): Query<PrePublishQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    program_bytes: Bytes,
) -> axum::response::Result<axum::Json<PrePublishResult>> {
    // User should not be able to print migration plans for a database that they do not own
    let database_identity = resolve_and_authenticate(&ctx, &name_or_identity, &auth).await?;
    let style = match style {
        PrettyPrintStyle::NoColor => AutoMigratePrettyPrintStyle::NoColor,
        PrettyPrintStyle::AnsiColor => AutoMigratePrettyPrintStyle::AnsiColor,
    };

    info!("planning migration for database {database_identity}");
    let migrate_plan = ctx
        .migrate_plan(
            DatabaseDef {
                database_identity,
                program_bytes,
                num_replicas: None,
                host_type,
                parent: None,
                organization: None,
            },
            style,
        )
        .await
        .map_err(log_and_500)?;

    match migrate_plan {
        MigratePlanResult::Success {
            old_module_hash,
            new_module_hash,
            breaks_client,
            plan,
            major_version_upgrade,
        } => {
            info!(
                "planned auto-migration of database {} from {} to {}",
                database_identity, old_module_hash, new_module_hash
            );
            let token = MigrationToken {
                database_identity,
                old_module_hash,
                new_module_hash,
            }
            .hash();

            Ok(PrePublishResult::AutoMigrate(PrePublishAutoMigrateResult {
                token,
                migrate_plan: plan,
                break_clients: breaks_client,
                major_version_upgrade,
            }))
        }
        MigratePlanResult::AutoMigrationError {
            error: e,
            major_version_upgrade,
        } => {
            info!("database {database_identity} needs manual migration");
            Ok(PrePublishResult::ManualMigrate(PrePublishManualMigrateResult {
                reason: e.to_string(),
                major_version_upgrade,
            }))
        }
    }
    .map(axum::Json)
}

/// Resolves the [`NameOrIdentity`] to a database identity and checks if the
/// `auth` identity owns the database.
async fn resolve_and_authenticate<S: ControlStateDelegate + Authorization>(
    ctx: &S,
    name_or_identity: &NameOrIdentity,
    auth: &SpacetimeAuth,
) -> axum::response::Result<Identity> {
    let database_identity = name_or_identity.resolve(ctx).await?;
    let database = worker_ctx_find_database(ctx, &database_identity)
        .await?
        .ok_or(NO_SUCH_DATABASE)?;

    ctx.authorize_action(auth.claims.identity, database.database_identity, Action::UpdateDatabase)
        .await?;

    Ok(database_identity)
}

#[derive(Deserialize)]
pub struct DeleteDatabaseParams {
    pub name_or_identity: NameOrIdentity,
}

pub async fn delete_database<S: ControlStateDelegate + Authorization>(
    State(ctx): State<S>,
    Path(DeleteDatabaseParams { name_or_identity }): Path<DeleteDatabaseParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = name_or_identity.resolve(&ctx).await?;
    let Some(_database) = worker_ctx_find_database(&ctx, &database_identity).await? else {
        return Ok(());
    };

    ctx.authorize_action(auth.claims.identity, database_identity, Action::DeleteDatabase)
        .await?;
    ctx.delete_database(&auth.claims.identity, &database_identity)
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
        .create_dns_record(&auth.claims.identity, &name.into(), &database_identity)
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

pub async fn set_names<S: ControlStateDelegate + Authorization>(
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

    let database = ctx
        .get_database_by_identity(&database_identity)
        .await
        .map_err(log_and_500)?;
    let Some(database) = database else {
        return Ok((
            StatusCode::NOT_FOUND,
            axum::Json(name::SetDomainsResult::DatabaseNotFound),
        ));
    };

    ctx.authorize_action(auth.claims.identity, database.database_identity, Action::RenameDatabase)
        .await
        .map_err(|e| match e {
            Unauthorized::Unauthorized { .. } => (
                StatusCode::UNAUTHORIZED,
                axum::Json(name::SetDomainsResult::NotYourDatabase {
                    database: database.database_identity,
                }),
            )
                .into(),
            Unauthorized::InternalError(e) => log_and_500(e),
        })?;

    for name in &validated_names {
        if ctx
            .lookup_database_identity(name.as_str())
            .await
            .map_err(log_and_500)?
            .is_some()
        {
            return Ok((
                StatusCode::BAD_REQUEST,
                axum::Json(name::SetDomainsResult::OtherError(format!(
                    "Cannot rename to {} because it already is in use.",
                    name.as_str()
                ))),
            ));
        }
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

#[derive(serde::Deserialize)]
pub struct TimestampParams {
    name_or_identity: NameOrIdentity,
}

/// Returns the database's view of the current time,
/// as a SATS-JSON encoded [`Timestamp`].
///
/// Takes a particular database's [`NameOrIdentity`] as an argument
/// because in a clusterized SpacetimeDB-cloud deployment,
/// this request will be routed to the node running the requested database.
async fn get_timestamp<S: ControlStateDelegate>(
    State(worker_ctx): State<S>,
    Path(TimestampParams { name_or_identity }): Path<TimestampParams>,
) -> axum::response::Result<impl IntoResponse> {
    let db_identity = name_or_identity.resolve(&worker_ctx).await?;

    let _database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", db_identity.to_hex());
            NO_SUCH_DATABASE
        })?;

    Ok(axum::Json(sats::serde::SerdeWrapper(Timestamp::now())).into_response())
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
    pub call_reducer_procedure_post: MethodRouter<S>,
    /// GET: /database/:name_or_identity/schema
    pub schema_get: MethodRouter<S>,
    /// GET: /database/:name_or_identity/logs
    pub logs_get: MethodRouter<S>,
    /// POST: /database/:name_or_identity/sql
    pub sql_post: MethodRouter<S>,
    /// POST: /database/:name_or_identity/pre-publish
    pub pre_publish: MethodRouter<S>,
    /// PUT: /database/:name_or_identity/reset
    pub db_reset: MethodRouter<S>,
    /// GET: /database/: name_or_identity/unstable/timestamp
    pub timestamp_get: MethodRouter<S>,
}

impl<S> Default for DatabaseRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + HasWebSocketOptions + Authorization + Clone + 'static,
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
            call_reducer_procedure_post: post(call::<S>),
            schema_get: get(schema::<S>),
            logs_get: get(logs::<S>),
            sql_post: post(sql::<S>),
            pre_publish: post(pre_publish::<S>),
            db_reset: put(reset::<S>),
            timestamp_get: get(get_timestamp::<S>),
        }
    }
}

impl<S> DatabaseRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + Authorization + Clone + 'static,
{
    pub fn into_router(self, ctx: S) -> axum::Router<S> {
        use axum::routing::any;

        let db_router = axum::Router::<S>::new()
            .route("/", self.db_put)
            .route("/", self.db_get)
            .route("/", self.db_delete)
            .route("/names", self.names_get)
            .route("/names", self.names_post)
            .route("/names", self.names_put)
            .route("/identity", self.identity_get)
            .route("/subscribe", self.subscribe_get)
            .route("/call/:reducer", self.call_reducer_procedure_post)
            .route("/schema", self.schema_get)
            .route("/logs", self.logs_get)
            .route("/sql", self.sql_post)
            .route("/unstable/timestamp", self.timestamp_get)
            .route("/pre_publish", self.pre_publish)
            .route("/reset", self.db_reset);

        let authed_root_router = axum::Router::new().route(
            "/",
            self.root_post.layer(axum::middleware::from_fn_with_state(
                ctx.clone(),
                anon_auth_middleware::<S>,
            )),
        );

        let authed_named_router = axum::Router::new()
            .nest("/:name_or_identity", db_router)
            .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>));

        // NOTE: HTTP route handlers are intentionally unauthenticated so they can accept
        // webhooks and other requests from outside the SpacetimeDB auth ecosystem.
        // This route must bypass `anon_auth_middleware` entirely so invalid/missing
        // Authorization headers do not trigger early rejection or attach SpacetimeAuth.
        // Keep these routes merged separately from the authenticated database router.
        let http_route_router = axum::Router::<S>::new()
            .route("/:name_or_identity/route", any(handle_http_route_root::<S>))
            .route("/:name_or_identity/route/", any(handle_http_route_root_slash::<S>))
            .route("/:name_or_identity/route/*path", any(handle_http_route::<S>));

        axum::Router::new()
            .merge(authed_root_router)
            .merge(authed_named_router)
            .merge(http_route_router)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::JwtAuthProvider;
    use crate::routes::subscribe::{HasWebSocketOptions, WebSocketOptions};
    use crate::{
        Action, Authorization, ControlStateReadAccess, ControlStateWriteAccess, MaybeMisdirected, Unauthorized,
    };
    use async_trait::async_trait;
    use axum::body::Body;
    use http::Request;
    use spacetimedb::auth::identity::{JwtError, JwtErrorKind, SpacetimeIdentityClaims};
    use spacetimedb::auth::token_validation::{TokenSigner, TokenValidationError, TokenValidator};
    use spacetimedb::client::ClientActorIndex;
    use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
    use spacetimedb::identity::AuthCtx;
    use spacetimedb::messages::control_db::{Database, Node, Replica};
    use spacetimedb_client_api_messages::name::{
        DomainName, InsertDomainResult, RegisterTldResult, SetDomainsResult, Tld,
    };
    use spacetimedb_paths::server::ModuleLogsDir;
    use spacetimedb_paths::FromPathUnchecked;
    use spacetimedb_schema::auto_migrate::{MigrationPolicy, PrettyPrintStyle};
    use tower::util::ServiceExt;
    #[derive(Clone, Default)]
    struct DummyValidator;

    #[async_trait]
    impl TokenValidator for DummyValidator {
        async fn validate_token(&self, _token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
            Err(TokenValidationError::Other(anyhow::anyhow!("unused")))
        }
    }

    #[derive(Clone)]
    struct DummyJwtProvider {
        validator: DummyValidator,
    }

    impl TokenSigner for DummyJwtProvider {
        fn sign<T: serde::Serialize>(&self, _claims: &T) -> Result<String, JwtError> {
            Err(JwtError::from(JwtErrorKind::InvalidSignature))
        }
    }

    impl JwtAuthProvider for DummyJwtProvider {
        type TV = DummyValidator;

        fn validator(&self) -> &Self::TV {
            &self.validator
        }

        fn local_issuer(&self) -> &str {
            "test"
        }

        fn public_key_bytes(&self) -> &[u8] {
            b""
        }
    }

    #[derive(Clone)]
    struct DummyState {
        jwt: DummyJwtProvider,
        client_actor_index: std::sync::Arc<ClientActorIndex>,
        module_logs_dir: ModuleLogsDir,
    }

    impl DummyState {
        fn new() -> Self {
            Self {
                jwt: DummyJwtProvider {
                    validator: DummyValidator,
                },
                client_actor_index: std::sync::Arc::new(ClientActorIndex::new()),
                module_logs_dir: ModuleLogsDir::from_path_unchecked(std::env::temp_dir()),
            }
        }
    }

    impl HasWebSocketOptions for DummyState {
        fn websocket_options(&self) -> WebSocketOptions {
            WebSocketOptions::default()
        }
    }

    #[async_trait]
    impl NodeDelegate for DummyState {
        type GetLeaderHostError = DummyLeaderError;

        fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
            Vec::new()
        }

        fn client_actor_index(&self) -> &ClientActorIndex {
            self.client_actor_index.as_ref()
        }

        type JwtAuthProviderT = DummyJwtProvider;
        fn jwt_auth_provider(&self) -> &Self::JwtAuthProviderT {
            &self.jwt
        }

        async fn leader(&self, _database_id: u64) -> Result<Host, Self::GetLeaderHostError> {
            Err(DummyLeaderError)
        }

        fn module_logs_dir(&self, _replica_id: u64) -> ModuleLogsDir {
            self.module_logs_dir.clone()
        }
    }

    #[derive(Debug)]
    struct DummyLeaderError;

    impl MaybeMisdirected for DummyLeaderError {
        fn is_misdirected(&self) -> bool {
            false
        }
    }

    impl std::fmt::Display for DummyLeaderError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("dummy leader error")
        }
    }

    impl From<DummyLeaderError> for ErrorResponse {
        fn from(_: DummyLeaderError) -> Self {
            (StatusCode::INTERNAL_SERVER_ERROR, "dummy leader error").into()
        }
    }

    #[async_trait]
    impl ControlStateReadAccess for DummyState {
        async fn get_node_id(&self) -> Option<u64> {
            None
        }
        async fn get_node_by_id(&self, _node_id: u64) -> anyhow::Result<Option<Node>> {
            Ok(None)
        }
        async fn get_nodes(&self) -> anyhow::Result<Vec<Node>> {
            Ok(Vec::new())
        }
        async fn get_database_by_id(&self, _id: u64) -> anyhow::Result<Option<Database>> {
            Ok(None)
        }
        async fn get_database_by_identity(&self, _database_identity: &Identity) -> anyhow::Result<Option<Database>> {
            Ok(None)
        }
        async fn get_databases(&self) -> anyhow::Result<Vec<Database>> {
            Ok(Vec::new())
        }
        async fn get_replica_by_id(&self, _id: u64) -> anyhow::Result<Option<Replica>> {
            Ok(None)
        }
        async fn get_replicas(&self) -> anyhow::Result<Vec<Replica>> {
            Ok(Vec::new())
        }
        async fn get_leader_replica_by_database(&self, _database_id: u64) -> Option<Replica> {
            None
        }
        async fn get_energy_balance(&self, _identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
            Ok(None)
        }
        async fn lookup_database_identity(&self, _domain: &str) -> anyhow::Result<Option<Identity>> {
            Ok(None)
        }
        async fn reverse_lookup(&self, _database_identity: &Identity) -> anyhow::Result<Vec<DomainName>> {
            Ok(Vec::new())
        }
        async fn lookup_namespace_owner(&self, _name: &str) -> anyhow::Result<Option<Identity>> {
            Ok(None)
        }
    }

    #[async_trait]
    impl ControlStateWriteAccess for DummyState {
        async fn publish_database(
            &self,
            _publisher: &Identity,
            _spec: DatabaseDef,
            _policy: MigrationPolicy,
        ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn migrate_plan(
            &self,
            _spec: DatabaseDef,
            _style: PrettyPrintStyle,
        ) -> anyhow::Result<MigratePlanResult> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn delete_database(
            &self,
            _caller_identity: &Identity,
            _database_identity: &Identity,
        ) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn reset_database(&self, _caller_identity: &Identity, _spec: DatabaseResetDef) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn add_energy(&self, _identity: &Identity, _amount: EnergyQuanta) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn withdraw_energy(&self, _identity: &Identity, _amount: EnergyQuanta) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn register_tld(&self, _identity: &Identity, _tld: Tld) -> anyhow::Result<RegisterTldResult> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn create_dns_record(
            &self,
            _owner_identity: &Identity,
            _domain: &DomainName,
            _database_identity: &Identity,
        ) -> anyhow::Result<InsertDomainResult> {
            Err(anyhow::anyhow!("unused"))
        }

        async fn replace_dns_records(
            &self,
            _database_identity: &Identity,
            _owner_identity: &Identity,
            _domain_names: &[DomainName],
        ) -> anyhow::Result<SetDomainsResult> {
            Err(anyhow::anyhow!("unused"))
        }
    }

    impl Authorization for DummyState {
        async fn authorize_action(
            &self,
            _subject: Identity,
            _database: Identity,
            _action: Action,
        ) -> Result<(), Unauthorized> {
            Err(Unauthorized::InternalError(anyhow::anyhow!("unused")))
        }

        async fn authorize_sql(&self, _subject: Identity, _database: Identity) -> Result<AuthCtx, Unauthorized> {
            Err(Unauthorized::InternalError(anyhow::anyhow!("unused")))
        }
    }

    /// Tests that requests to user-defined routes under `/database/:name-or-identity/routes`
    /// bypass the usual SpacetimeDB auth middleware,
    /// and accept requests with `Authorization` headers that SpacetimeDB would treat as malformed.
    ///
    /// This behavior is necessary to allow HTTP handlers to accept requests from non-SpacetimeDB-ecosystem clients,
    /// e.g. for the purposes of handling webhooks.
    #[tokio::test]
    async fn http_route_bypasses_auth_middleware() {
        let state = DummyState::new();
        let app = DatabaseRoutes::<DummyState>::default()
            .into_router(state.clone())
            .with_state(state);

        let request = Request::builder()
            .method(http::Method::POST)
            .uri("/not-a-database/route/health")
            .header(http::header::AUTHORIZATION, "Bearer not-a-jwt")
            .body(Body::from("payload"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        // We'll get this error message out of the stack:
        // - `find_module_and_database`
        // - `find_leader_and_database`
        // - `name_or_identity.resolve(worker_ctx)` -> `NameOrIdentity::resolve`
        assert_eq!(body, "`not-a-database` not found");
    }
}
