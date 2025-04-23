use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::to_bytes;
use axum::response::IntoResponse;
use futures::{stream, Sink};
use futures::{SinkExt, Stream};
use http::StatusCode;
use pgwire::api::auth::{
    finish_authentication, save_startup_parameters_to_metadata, DefaultServerParameterProvider, LoginInfo,
    StartupHandler,
};
use pgwire::api::copy::NoopCopyHandler;
use pgwire::api::portal::Format;
use pgwire::api::query::{PlaceholderExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::results::{DataRowEncoder, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::{ClientInfo, Type};
use pgwire::api::{NoopErrorHandler, METADATA_DATABASE, METADATA_USER};
use pgwire::api::{PgWireConnectionState, PgWireServerHandlers};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use pgwire::messages::startup::Authentication;
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage};
use pgwire::tokio::process_socket;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::PrivateKeyDer;
use spacetimedb_client_api::auth::{validate_token, SpacetimeAuth};
use spacetimedb_client_api::routes::database;
use spacetimedb_client_api::routes::database::{SqlParams, SqlQueryParams};
use spacetimedb_client_api::{ControlStateReadAccess, ControlStateWriteAccess, NodeDelegate};
use spacetimedb_client_api_messages::http::SqlStmtResult;
use spacetimedb_client_api_messages::name::DatabaseName;
use spacetimedb_lib::sats::satn::{PsqlPrintFmt, Satn};
use spacetimedb_lib::sats::ArrayValue;
use spacetimedb_lib::version::spacetimedb_lib_version;
use spacetimedb_lib::{
    AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue, TimeDuration, Timestamp,
};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

#[derive(Error, Debug)]
enum PgError {
    #[error("(metadata) {0}")]
    MetadataError(anyhow::Error),
    #[error("(Sql) {0}")]
    Sql(String),
    #[error("Database name is required")]
    DatabaseNameRequired,
    #[error(transparent)]
    Pg(#[from] PgWireError),
    #[error(transparent)]
    RcGen(#[from] rcgen::Error),
    #[error(transparent)]
    Pem(#[from] rustls_pki_types::pem::Error),
    #[error(transparent)]
    RustTls(#[from] rustls::Error),
    #[error("Special type with format {0} is invalid for {1}")]
    SpecialTypeInvalid(PsqlPrintFmt, String),
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
    auth: SpacetimeAuth,
}

fn type_of(schema: &ProductType, ty: &ProductTypeElement) -> Type {
    let format = PsqlPrintFmt::use_fmt(schema, ty, ty.name());
    match &ty.algebraic_type {
        AlgebraicType::String => Type::VARCHAR,
        AlgebraicType::Bool => Type::BOOL,
        AlgebraicType::I8 | AlgebraicType::U8 | AlgebraicType::I16 => Type::INT2,
        AlgebraicType::U16 | AlgebraicType::I32 => Type::INT4,
        AlgebraicType::U32 | AlgebraicType::I64 => Type::INT8,
        AlgebraicType::U64 | AlgebraicType::I128 | AlgebraicType::U128 | AlgebraicType::I256 | AlgebraicType::U256 => {
            Type::NUMERIC
        }
        AlgebraicType::F32 => Type::FLOAT4,
        AlgebraicType::F64 => Type::FLOAT8,
        AlgebraicType::Array(ty) => match *ty.elem_ty {
            AlgebraicType::String => Type::VARCHAR_ARRAY,
            AlgebraicType::Bool => Type::BOOL_ARRAY,
            AlgebraicType::U8 => Type::BYTEA_ARRAY,
            AlgebraicType::I8 | AlgebraicType::I16 => Type::INT2_ARRAY,
            AlgebraicType::U16 | AlgebraicType::I32 => Type::INT4_ARRAY,
            AlgebraicType::U32 | AlgebraicType::I64 => Type::INT8_ARRAY,
            AlgebraicType::U64
            | AlgebraicType::I128
            | AlgebraicType::U128
            | AlgebraicType::I256
            | AlgebraicType::U256 => Type::NUMERIC_ARRAY,
            _ => Type::ANYARRAY,
        },
        AlgebraicType::Product(_) => match format {
            PsqlPrintFmt::Hex => Type::BYTEA_ARRAY,
            PsqlPrintFmt::Timestamp => Type::TIMESTAMP,
            PsqlPrintFmt::Duration => Type::INTERVAL,
            _ => Type::JSON,
        },
        x if x.as_sum().map(|x| x.is_simple_enum()).unwrap_or(false) => Type::ANYENUM,
        _ => Type::UNKNOWN,
    }
}

fn encode_value(
    encoder: &mut DataRowEncoder,
    schema: &ProductType,
    ty: &ProductTypeElement,
    value: &AlgebraicValue,
) -> Result<(), PgError> {
    let format = PsqlPrintFmt::use_fmt(schema, ty, ty.name());

    match value {
        AlgebraicValue::Bool(x) => encoder.encode_field(x)?,
        AlgebraicValue::I8(x) => encoder.encode_field(x)?,
        AlgebraicValue::U8(x) => encoder.encode_field(&(*x as i16))?,
        AlgebraicValue::I16(x) => encoder.encode_field(x)?,
        AlgebraicValue::U16(x) => encoder.encode_field(&(*x as u32))?,
        AlgebraicValue::I32(x) => encoder.encode_field(x)?,
        AlgebraicValue::U32(x) => encoder.encode_field(x)?,
        AlgebraicValue::I64(x) => encoder.encode_field(&x)?,
        AlgebraicValue::U64(x) => encoder.encode_field(&x.to_string())?,
        AlgebraicValue::I128(x) => {
            let x = x.0;
            encoder.encode_field(&(x.to_string()))?
        }
        AlgebraicValue::U128(x) => {
            let x = x.0;
            encoder.encode_field(&x.to_string())?;
        }
        AlgebraicValue::I256(x) => encoder.encode_field(&(x.to_string()))?,
        AlgebraicValue::U256(x) => encoder.encode_field(&x.to_string())?,
        AlgebraicValue::F32(x) => encoder.encode_field(&x.into_inner())?,
        AlgebraicValue::F64(x) => encoder.encode_field(&x.into_inner())?,
        AlgebraicValue::String(x) => encoder.encode_field(&x.to_string())?,
        AlgebraicValue::Array(x) => match x {
            ArrayValue::Bool(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::I8(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::U8(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::I16(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::I32(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::U32(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::I64(x) => {
                encoder.encode_field(&x.as_ref())?;
            }
            ArrayValue::F32(x) => {
                let x = x.iter().map(|x| x.into_inner()).collect::<Vec<_>>();
                encoder.encode_field(&x)?;
            }
            ArrayValue::F64(x) => {
                let x = x.iter().map(|x| x.into_inner()).collect::<Vec<_>>();
                encoder.encode_field(&x)?;
            }
            ArrayValue::String(x) => {
                let x = x.iter().map(|x| x.as_ref()).collect::<Vec<_>>();
                encoder.encode_field(&x)?;
            }
            _ => {
                let json = serde_json::to_string(&value).unwrap();
                encoder.encode_field(&json)?;
            }
        },
        AlgebraicValue::Product(x) => match (format, x.as_special_value_raw()) {
            (PsqlPrintFmt::Hex, Some(AlgebraicValue::U128(x))) => encoder.encode_field(&x.0.to_be_bytes())?,
            (PsqlPrintFmt::Hex, Some(AlgebraicValue::U256(x))) => encoder.encode_field(&x.to_be_bytes())?,
            (PsqlPrintFmt::Timestamp, Some(AlgebraicValue::I64(x))) => {
                encoder.encode_field(&Timestamp::from_micros_since_unix_epoch(*x).to_rfc3339()?)?
            }
            (PsqlPrintFmt::Duration, Some(AlgebraicValue::I64(x))) => {
                encoder.encode_field(&TimeDuration::from_micros(*x).to_iso8601())?
            }
            (PsqlPrintFmt::Satn, Some(..))
            | (PsqlPrintFmt::Hex | PsqlPrintFmt::Timestamp | PsqlPrintFmt::Duration, _) => {
                return Err(PgError::SpecialTypeInvalid(format, value.to_satn()))
            }
            (PsqlPrintFmt::Satn, None) => {
                let json = serde_json::to_string(&value).unwrap();

                encoder.encode_field(&json)?
            }
        },
        x => encoder.encode_field(&x.to_satn())?,
    }

    Ok(())
}

fn to_rows(
    stmt: SqlStmtResult<ProductValue>,
    header: Arc<Vec<FieldInfo>>,
) -> Result<impl Stream<Item = PgWireResult<DataRow>>, PgError> {
    let mut results = Vec::with_capacity(stmt.rows.len());

    for row in stmt.rows {
        let mut encoder = DataRowEncoder::new(header.clone());

        for (idx, ty) in stmt.schema.elements.iter().enumerate() {
            let value = row.get_field(idx, None).unwrap();

            encode_value(&mut encoder, &stmt.schema, ty, value)?;
        }
        results.push(encoder.finish());
    }
    Ok(stream::iter(results))
}

fn row_desc_from_stmt(stmt: &SqlStmtResult<ProductValue>, format: &Format) -> Vec<FieldInfo> {
    let mut field_info = Vec::with_capacity(stmt.schema.elements.len());
    for (idx, ty) in stmt.schema.elements.iter().enumerate() {
        let field_name = ty.name.clone().map(Into::into).unwrap_or_else(|| format!("col {idx}"));
        let field_type = type_of(&stmt.schema, ty);
        let field_desc = FieldInfo::new(field_name, None, None, field_type, format.format_for(idx));
        field_info.push(field_desc);
    }
    field_info
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
            Err(PgError::Sql(format!("{err}")))
        }
    }
}

struct PgSpacetimeDB<T> {
    ctx: Arc<T>,
    cached: Mutex<Metadata>,
    parameter_provider: DefaultServerParameterProvider,
}

impl<T: ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate> PgSpacetimeDB<T> {
    async fn exe_sql<'a>(&self, query: String) -> PgWireResult<Vec<Response<'a>>> {
        let params = self.cached.lock().await;
        let db = SqlParams {
            name_or_identity: database::NameOrIdentity::Name(DatabaseName(params.database.clone())),
        };

        let sql = match response(
            database::sql_direct(
                self.ctx.clone(),
                db,
                SqlQueryParams { confirmed: true },
                params.auth.identity,
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
            let header = Arc::new(row_desc_from_stmt(&sql_result, &Format::UnifiedText));

            let tag = Tag::new(&stats(&sql_result));

            if sql_result.rows.is_empty() {
                result.push(Response::EmptyQuery);
            } else {
                let rows = to_rows(sql_result, header.clone())?;
                result.push(Response::Query(QueryResponse::new(header, rows)));
            }
            result.push(Response::Execution(tag));
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
                save_startup_parameters_to_metadata(client, startup);
                client.set_state(PgWireConnectionState::AuthenticationInProgress);

                let login_info = LoginInfo::from_client_info(client);

                log::debug!("PG: Login info: {login_info:?}");

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
                        .ok_or_else(|| PgError::MetadataError(anyhow::anyhow!("Missing parameter: {}", param)))
                };

                let user = param(METADATA_USER)?;
                let database = param(METADATA_DATABASE)?;
                let pwd = pwd.into_password()?;
                if let Ok(application_name) = param("application_name") {
                    log::info!("PG: Connecting to database: {user}@{database}, by {application_name}",);
                } else {
                    log::info!("PG: Connecting to database: {user}@{database}");
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

                let auth = match validate_token(&self.ctx, &pwd.password).await {
                    Ok(claims) => response(SpacetimeAuth::from_claims(&self.ctx, claims), &database).await?,
                    Err(err) => {
                        let err = ErrorInfo::new("FATAL".to_owned(), "28P01".to_owned(), err.to_string());
                        return close_client(client, err).await;
                    }
                };

                log::info!(
                    "PG: Connected to database: {user}@{database} using identity `{}`",
                    auth.identity
                );

                let metadata = Metadata { database, auth };
                self.cached.lock().await.clone_from(&metadata);
                finish_authentication(client, &self.parameter_provider).await?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[async_trait]
impl<T: Sync + Send + ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate> SimpleQueryHandler
    for PgSpacetimeDB<T>
{
    async fn do_query<'a, C>(&self, _client: &mut C, query: &'a str) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        self.exe_sql(query.to_string()).await
    }
}

#[derive(Clone)]
struct PgSpacetimeDBFactory<T> {
    handler: Arc<PgSpacetimeDB<T>>,
}

impl<T> PgSpacetimeDBFactory<T> {
    pub fn new(ctx: Arc<T>, auth: SpacetimeAuth) -> Self {
        let mut parameter_provider = DefaultServerParameterProvider::default();
        parameter_provider.server_version = format!("spacetime {}", spacetimedb_lib_version());

        Self {
            handler: Arc::new(PgSpacetimeDB {
                ctx,
                cached: Mutex::new(Metadata {
                    // This is a placeholder, it will be set in the startup handler
                    database: "".to_string(),
                    auth,
                }),
                parameter_provider,
            }),
        }
    }
}

impl<T: Sync + Send + ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate> PgWireServerHandlers
    for PgSpacetimeDBFactory<T>
{
    type StartupHandler = PgSpacetimeDB<T>;
    type SimpleQueryHandler = PgSpacetimeDB<T>;
    type ExtendedQueryHandler = PlaceholderExtendedQueryHandler;
    type CopyHandler = NoopCopyHandler;
    type ErrorHandler = NoopErrorHandler;

    fn simple_query_handler(&self) -> Arc<Self::SimpleQueryHandler> {
        self.handler.clone()
    }

    fn extended_query_handler(&self) -> Arc<Self::ExtendedQueryHandler> {
        Arc::new(PlaceholderExtendedQueryHandler)
    }

    fn startup_handler(&self) -> Arc<Self::StartupHandler> {
        self.handler.clone()
    }

    fn copy_handler(&self) -> Arc<Self::CopyHandler> {
        Arc::new(NoopCopyHandler)
    }

    fn error_handler(&self) -> Arc<Self::ErrorHandler> {
        Arc::new(NoopErrorHandler)
    }
}

fn setup_tls<T>(_ctx: &T, private_key: &[u8]) -> Result<TlsAcceptor, PgError> {
    let private: PrivateKeyDer = PrivateKeyDer::from_pem_slice(private_key)?;

    let keypair = KeyPair::from_der_and_sign_algo(&private, &rcgen::PKCS_ECDSA_P256_SHA256)?;

    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])?;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "localhost");
    let cert = params.self_signed(&keypair)?;
    let cert_der = cert.der().clone();

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], private)?;

    config.alpn_protocols = vec![b"postgresql".to_vec(), b"spacetime".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub async fn start_pg<T: ControlStateReadAccess + ControlStateWriteAccess + NodeDelegate + 'static>(
    mut shutdown: watch::Receiver<()>,
    ctx: Arc<T>,
    listen_address: &str,
    private_key: &[u8],
) {
    let tls_acceptor = Arc::new(setup_tls(&ctx, private_key).unwrap());

    let auth = SpacetimeAuth::alloc(&ctx).await.unwrap();
    let factory = Arc::new(PgSpacetimeDBFactory::new(ctx, auth));

    let server_addr = format!("{}:5432", listen_address.split(':').next().unwrap());
    let tcp = TcpListener::bind(server_addr).await.unwrap();

    log::debug!(
        "PG: Starting SpacetimeDB Protocol listening on {}",
        tcp.local_addr().unwrap()
    );
    loop {
        tokio::select! {
            accept_result = tcp.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let tls_acceptor_ref = tls_acceptor.clone();
                        let factory_ref = factory.clone();
                        tokio::spawn(async move {
                            process_socket(stream, Some(tls_acceptor_ref),  factory_ref).await.inspect_err(|err|{
                                log::error!("PG: Error processing socket: {err:?}");
                            })
                        });
                    }
                    Err(e) => {
                       log::error!("PG: Accept error: {e}");
                    }
                }
            }
            _ = shutdown.changed() => {
                log::info!("PG: Shutting down PostgreSQL server.");
                break;
            }
        }
    }
}
