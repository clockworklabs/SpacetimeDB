use std::fmt::Debug;
use std::sync::Arc;

use crate::encoder::{row_desc, PsqlFormatter};
use async_trait::async_trait;
use axum::body::to_bytes;
use axum::response::IntoResponse;
use futures::{stream, Sink};
use futures::{SinkExt, Stream};
use http::StatusCode;
use pgwire::api::auth::{
    finish_authentication, protocol_negotiation, save_startup_parameters_to_metadata, DefaultServerParameterProvider,
    LoginInfo, StartupHandler,
};
use pgwire::api::portal::Format;
use pgwire::api::query::SimpleQueryHandler;
use pgwire::api::results::{DataRowEncoder, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::{ClientInfo, METADATA_DATABASE};
use pgwire::api::{PgWireConnectionState, PgWireServerHandlers};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use pgwire::messages::startup::Authentication;
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage};
use pgwire::tokio::process_socket;
use spacetimedb_client_api::auth::validate_token;
use spacetimedb_client_api::routes::database;
use spacetimedb_client_api::routes::database::{SqlParams, SqlQueryParams};
use spacetimedb_client_api::{ControlStateReadAccess, ControlStateWriteAccess, NodeDelegate};
use spacetimedb_client_api_messages::http::SqlStmtResult;
use spacetimedb_client_api_messages::name::DatabaseName;
use spacetimedb_lib::sats::satn::{PsqlClient, TypedSerializer};
use spacetimedb_lib::sats::{satn, Serialize, Typespace};
use spacetimedb_lib::version::spacetimedb_lib_version;
use spacetimedb_lib::{Identity, ProductValue};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Notify};

#[derive(Error, Debug)]
pub(crate) enum PgError {
    #[error("(metadata) {0}")]
    MetadataError(anyhow::Error),
    #[error("(Sql) {0}")]
    Sql(String),
    #[error("Database name is required")]
    DatabaseNameRequired,
    #[error(transparent)]
    Pg(#[from] PgWireError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<PgError> for PgWireError {
    fn from(err: PgError) -> Self {
        if let PgError::Pg(err) = err {
            err
        } else {
            PgWireError::ApiError(Box::new(err))
        }
    }
}

#[derive(Clone)]
struct Metadata {
    database: String,
    caller_identity: Identity,
}

pub(crate) fn to_rows(
    stmt: SqlStmtResult<ProductValue>,
    header: Arc<Vec<FieldInfo>>,
) -> Result<impl Stream<Item = PgWireResult<DataRow>>, PgError> {
    let mut results = Vec::with_capacity(stmt.rows.len());
    let ty = Typespace::EMPTY.with_type(&stmt.schema);

    for row in stmt.rows {
        let mut encoder = DataRowEncoder::new(header.clone());

        for (idx, value) in ty.with_values(&row).enumerate() {
            let ty = satn::PsqlType {
                client: PsqlClient::Postgres,
                tuple: ty.ty(),
                field: &ty.ty().elements[idx],
                idx,
            };
            let mut fmt = PsqlFormatter { encoder: &mut encoder };
            value.serialize(TypedSerializer { ty: &ty, f: &mut fmt })?;
        }
        results.push(encoder.finish());
    }
    Ok(stream::iter(results))
}

fn stats(stmt: &SqlStmtResult<ProductValue>) -> String {
    let mut info = Vec::new();
    if stmt.stats.rows_inserted != 0 {
        info.push(format!("inserted: {}", stmt.stats.rows_inserted));
    }
    if stmt.stats.rows_deleted != 0 {
        info.push(format!("deleted: {}", stmt.stats.rows_deleted));
    }
    if stmt.stats.rows_updated != 0 {
        info.push(format!("updated: {}", stmt.stats.rows_updated));
    }
    info.push(format!(
        "server: {:.2?}",
        std::time::Duration::from_micros(stmt.total_duration_micros)
    ));

    info.join(", ")
}

struct ResponseWrapper<T>(T);
impl<T> IntoResponse for ResponseWrapper<T> {
    fn into_response(self) -> axum::response::Response {
        unreachable!("Blank impl to satisfy IntoResponse")
    }
}

async fn response<T>(res: axum::response::Result<T>, database: &str) -> Result<T, PgError> {
    match res.map(ResponseWrapper) {
        Ok(sql) => Ok(sql.0),
        err => {
            let res = err.into_response();
            if res.status() == StatusCode::NOT_FOUND {
                log::error!("PG: Database not found: {database}");
                return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                    "FATAL".to_string(),
                    "3D000".to_string(),
                    format!("database \"{database}\" does not exist"),
                )))
                .into());
            }
            let bytes = to_bytes(res.into_body(), usize::MAX)
                .await
                .map_err(|err| PgWireError::ApiError(Box::new(err)))?;
            let err = String::from_utf8_lossy(&bytes);
            log::error!("PG: Error for database {database}: {err}");
            Err(PgError::Sql(format!("{err}")))
        }
    }
}

struct PgSpacetimeDB<T> {
    ctx: T,
    cached: Mutex<Option<Metadata>>,
    parameter_provider: DefaultServerParameterProvider,
}

impl<T: ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate + Clone> PgSpacetimeDB<T> {
    async fn exe_sql(&self, query: String) -> PgWireResult<Vec<Response>> {
        let params = self.cached.lock().await.clone().unwrap();
        let db = SqlParams {
            name_or_identity: database::NameOrIdentity::Name(DatabaseName(params.database.clone())),
        };

        let sql = match response(
            database::sql_direct(
                self.ctx.clone(),
                db,
                SqlQueryParams { confirmed: true },
                params.caller_identity,
                query.to_string(),
            )
            .await,
            &params.database,
        )
        .await
        {
            Ok(sql) => sql,
            Err(PgError::Pg(PgWireError::UserError(err))) => {
                return Ok(vec![Response::Error(err)]);
            }
            Err(err) => {
                return Err(err.into());
            }
        };

        let mut result = Vec::with_capacity(sql.len());
        for sql_result in sql {
            let header = row_desc(&sql_result.schema, &Format::UnifiedText);
            if sql_result.rows.is_empty() && !query.to_uppercase().contains("SELECT") {
                let tag = Tag::new(&stats(&sql_result));
                result.push(Response::Execution(tag));
            } else {
                let rows = to_rows(sql_result, header.clone())?;
                let q = QueryResponse::new(header, rows);
                result.push(Response::Query(q));
            }
        }
        Ok(result)
    }
}

async fn close_client<C, E>(client: &mut C, err: E) -> PgWireResult<()>
where
    C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send,
    C::Error: Debug,
    PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    pgwire::messages::response::ErrorResponse: From<E>,
{
    let err = pgwire::messages::response::ErrorResponse::from(err);
    client.feed(PgWireBackendMessage::ErrorResponse(err)).await?;
    client.close().await?;
    Ok(())
}

#[async_trait]
impl<T: Sync + Send + ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate> StartupHandler
    for PgSpacetimeDB<T>
{
    async fn on_startup<C>(&self, client: &mut C, message: PgWireFrontendMessage) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        match message {
            PgWireFrontendMessage::Startup(ref startup) => {
                protocol_negotiation(client, startup).await?;
                save_startup_parameters_to_metadata(client, startup);
                client.set_state(PgWireConnectionState::AuthenticationInProgress);

                let login_info = LoginInfo::from_client_info(client);

                if login_info.database().is_none() {
                    return Err(PgError::DatabaseNameRequired.into());
                }

                client
                    .send(PgWireBackendMessage::Authentication(Authentication::CleartextPassword))
                    .await?;
            }
            PgWireFrontendMessage::PasswordMessageFamily(pwd) => {
                let params = client.metadata();
                let param = |param: &str| {
                    params
                        .get(param)
                        .map(String::from)
                        .ok_or_else(|| PgError::MetadataError(anyhow::anyhow!("Missing parameter: {param}")))
                };

                // We don't support `METADATA_USER` because we don't have a user management system.
                let database = param(METADATA_DATABASE)?;
                let pwd = pwd.into_password()?;
                if let Ok(application_name) = param("application_name") {
                    log::info!("PG: Connecting to database: {database}, by {application_name}",);
                } else {
                    log::info!("PG: Connecting to database: {database}");
                }

                let name = database::NameOrIdentity::Name(DatabaseName(database.clone()));
                match response(name.resolve(&self.ctx).await, &database).await {
                    Ok(identity) => identity,
                    Err(PgError::Pg(PgWireError::UserError(err))) => {
                        return close_client(client, *err).await;
                    }
                    Err(err) => {
                        return Err(err.into());
                    }
                };

                let caller_identity = match validate_token(&self.ctx, &pwd.password).await {
                    Ok(claims) => claims.identity,
                    Err(err) => {
                        log::error!(
                            "PG: Authentication failed for identity `{}` on database {database}: {err}",
                            pwd.password
                        );
                        let err = ErrorInfo::new("FATAL".to_owned(), "28P01".to_owned(), err.to_string());
                        return close_client(client, err).await;
                    }
                };

                log::info!("PG: Connected to database: {database} using identity `{caller_identity}`");

                let metadata = Metadata {
                    database,
                    caller_identity,
                };
                self.cached.lock().await.clone_from(&Some(metadata));
                finish_authentication(client, &self.parameter_provider).await?;
            }
            // The other messages are for features not supported by SpacetimeDB, that are rejected by the parser.
            // This includes TLS negotiation - any TLS negotiation done with the client will happen before
            // this point, and because we pass `tls_acceptor: None` for `process_socket()`, pgwire will reject
            // TLS for us.
            _ => {
                unreachable!("Unsupported startup message: {message:?}");
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<T: Sync + Send + ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate + Clone> SimpleQueryHandler
    for PgSpacetimeDB<T>
{
    async fn do_query<C>(&self, _client: &mut C, query: &str) -> PgWireResult<Vec<Response>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        self.exe_sql(query.to_string()).await
    }
}

#[derive(Clone)]
pub struct PgSpacetimeDBFactory<T> {
    handler: Arc<PgSpacetimeDB<T>>,
}

impl<T> PgSpacetimeDBFactory<T> {
    pub fn new(ctx: T) -> Self {
        let mut parameter_provider = DefaultServerParameterProvider::default();
        parameter_provider.server_version = format!("spacetime {}", spacetimedb_lib_version());

        Self {
            handler: Arc::new(PgSpacetimeDB {
                ctx,
                // This is a placeholder, it will be set in the startup handler
                cached: None.into(),
                parameter_provider,
            }),
        }
    }
}

impl<T: Sync + Send + ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate + Clone> PgWireServerHandlers
    for PgSpacetimeDBFactory<T>
{
    fn simple_query_handler(&self) -> Arc<impl SimpleQueryHandler> {
        self.handler.clone()
    }

    // TODO: fn extended_query_handler(&self) -> Arc<impl ExtendedQueryHandler> {}

    fn startup_handler(&self) -> Arc<impl StartupHandler> {
        self.handler.clone()
    }
}

pub async fn start_pg<T: ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate + Clone + 'static>(
    shutdown: Arc<Notify>,
    ctx: T,
    tcp: TcpListener,
) {
    let factory = Arc::new(PgSpacetimeDBFactory::new(ctx));

    log::debug!(
        "PG: Starting SpacetimeDB Protocol listening on {}",
        tcp.local_addr().unwrap()
    );
    loop {
        tokio::select! {
            accept_result = tcp.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let factory_ref = factory.clone();
                        tokio::spawn(async move {
                            process_socket(stream, None, factory_ref).await.inspect_err(|err|{
                                log::error!("PG: Error processing socket: {err:?}");
                            })
                        });
                    }
                    Err(e) => {
                       log::error!("PG: Accept error: {e}");
                    }
                }
            }
            _ = shutdown.notified() => {
                log::info!("PG: Shutting down PostgreSQL server.");
                break;
            }
        }
    }
}
