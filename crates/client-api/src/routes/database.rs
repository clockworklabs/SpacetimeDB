use crate::auth::{
    anon_auth_middleware, SpacetimeAuth, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros, SpacetimeIdentity,
    SpacetimeIdentityToken,
};
use crate::routes::subscribe::generate_random_connection_id;
use crate::util::{ByteStringBody, NameOrIdentity};
use crate::{log_and_500, ControlStateDelegate, DatabaseDef, NodeDelegate};
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::handler::Handler;
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
use spacetimedb_client_api_messages::name::{self, PublishOp, PublishResult};
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::sats;

use super::domain::DomainParsingRejection;
use super::identity::IdentityForUrl;
use super::subscribe::handle_websocket;

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
    TypedHeader(content_type): TypedHeader<headers::ContentType>,
    ByteStringBody(body): ByteStringBody,
) -> axum::response::Result<impl IntoResponse> {
    if content_type != headers::ContentType::json() {
        return Err(axum::extract::rejection::MissingJsonContentType::default().into());
    }
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
    Ok(identity.identity().to_string())
}

pub async fn get_name<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(ReverseDNSParams { name_or_identity }): Path<ReverseDNSParams>,
) -> axum::response::Result<impl IntoResponse> {
    let names = match name_or_identity {
        NameOrIdentity::Identity(database_identity) => {
            let database_identity = Identity::from(database_identity);
            ctx.reverse_lookup(&database_identity).map_err(log_and_500)?
        }
        NameOrIdentity::Name(name) => {
            vec![name.parse().map_err(|_| DomainParsingRejection)?]
        }
    };

    let response = name::ReverseDNSResponse { names };
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
}

pub async fn publish<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams { name_or_identity }): Path<PublishDatabaseParams>,
    Query(PublishDatabaseQueryParams { clear }): Query<PublishDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
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
    // this field is named such because that's what the route is under.
    // TODO: find out if there's a reason this can't be specified as a domain name
    name_or_identity: IdentityForUrl,
}

pub async fn delete_database<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(DeleteDatabaseParams {
        name_or_identity: database_identity,
    }): Path<DeleteDatabaseParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = Identity::from(database_identity);

    ctx.delete_database(&auth.identity, &database_identity)
        .await
        .map_err(log_and_500)?;

    Ok(())
}

#[derive(Deserialize)]
pub struct AddNameParams {
    // this field is named such because that's what the route is under.
    // TODO: find out if there's a reason this can't be specified as a domain name
    name_or_identity: IdentityForUrl,
}

pub async fn add_name<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(AddNameParams { name_or_identity }): Path<AddNameParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    domain: String,
) -> axum::response::Result<impl IntoResponse> {
    let database_identity = Identity::from(name_or_identity);

    let database = ctx
        .get_database_by_identity(&database_identity)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    // FIXME: fairly sure this check is redundant
    if database.owner_identity != auth.identity {
        return Err((StatusCode::UNAUTHORIZED, "Identity does not own database.").into());
    }

    let domain = domain.parse().map_err(|_| DomainParsingRejection)?;
    let response = ctx
        .create_dns_record(&auth.identity, &domain, &database_identity)
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

pub fn routes<S>(ctx: S, proxy: impl ControlProxy<S>) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{delete, get, post, put};
    let publish = publish::<S>.layer(DefaultBodyLimit::disable());
    let db_router = axum::Router::<S>::new()
        .route("/", put(publish.clone()))
        .route("/", get(db_info::<S>))
        .route("/", delete(delete_database::<S>))
        .route("/names", get(get_name::<S>))
        .route("/names", post(add_name::<S>))
        .route("/identity", get(get_identity::<S>))
        .route("/subscribe", get(handle_websocket::<S>.layer(proxy.get())))
        .route("/call/:reducer", post(call::<S>.layer(proxy.get())))
        .route("/schema", get(schema::<S>.layer(proxy.get())))
        .route("/logs", get(logs::<S>.layer(proxy.get())))
        .route("/sql", post(sql::<S>.layer(proxy.get())));

    axum::Router::new()
        .route("/", post(publish))
        .nest("/:name_or_identity", db_router)
        .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
}

pub trait ControlProxy<S: Clone + Send + 'static> {
    type Layer<H: Handler<T, S>, T: 'static>: tower_layer::Layer<
            axum::handler::HandlerService<H, T, S>,
            Service: Clone
                         + Send
                         + 'static
                         + tower_service::Service<
                axum::extract::Request,
                Response: IntoResponse,
                Future: Send,
                Error = std::convert::Infallible,
            >,
        > + Clone
        + Send
        + 'static;
    fn get<H: Handler<T, S>, T: 'static>(&self) -> Self::Layer<H, T>;
}

impl<S: Clone + Send + Sync + 'static> ControlProxy<S> for () {
    type Layer<H: Handler<T, S>, T: 'static> = tower_layer::Identity;
    fn get<H: Handler<T, S>, T: 'static>(&self) -> Self::Layer<H, T> {
        tower_layer::Identity::new()
    }
}
