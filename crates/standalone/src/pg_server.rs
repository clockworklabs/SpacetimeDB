use std::fmt::Debug;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;

use crate::StandaloneEnv;
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
use spacetimedb_client_api::auth::{validate_token, JwtAuthProvider, SpacetimeAuth};
use spacetimedb_client_api::routes::database;
use spacetimedb_client_api::routes::database::{SqlParams, SqlQueryParams};
use spacetimedb_client_api::NodeDelegate;
use spacetimedb_client_api_messages::http::SqlStmtResult;
use spacetimedb_client_api_messages::name::DatabaseName;
use spacetimedb_lib::sats::satn::Satn;
use spacetimedb_lib::sats::ArrayValue;
use spacetimedb_lib::version::spacetimedb_lib_version;
use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductValue};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

#[derive(Error, Debug)]
enum PgError {
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error("(metadata) {0}")]
    MetadataError(anyhow::Error),
    #[error("(Sql) {0}")]
    Sql(String),
    #[error("Database name is required")]
    DatabaseNameRequired,
}

impl From<PgError> for PgWireError {
    fn from(err: PgError) -> Self {
        PgWireError::ApiError(Box::new(err))
    }
}

#[derive(Clone)]
struct Metadata {
    database: String,
    auth: SpacetimeAuth,
}

fn type_of(ty: &AlgebraicType) -> Type {
    match ty {
        AlgebraicType::String => Type::VARCHAR,
        AlgebraicType::Bool => Type::BOOL,
        AlgebraicType::U8 => Type::BYTEA,
        AlgebraicType::I8 | AlgebraicType::I16 => Type::INT2,
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
        x if x.as_sum().map(|x| x.is_simple_enum()).unwrap_or(false) => Type::ANYENUM,
        _ => Type::UNKNOWN,
    }
}

fn to_rows(
    stmt: SqlStmtResult<ProductValue>,
    header: Arc<Vec<FieldInfo>>,
) -> impl Stream<Item = PgWireResult<DataRow>> {
    let ncols = header.len();
    let mut results = Vec::with_capacity(stmt.rows.len());

    for row in stmt.rows {
        let mut encoder = DataRowEncoder::new(header.clone());

        for idx in 0..ncols {
            let value = row.get_field(idx, None).unwrap();
            match value {
                AlgebraicValue::Bool(x) => encoder.encode_field(x).unwrap(),
                AlgebraicValue::I8(x) => encoder.encode_field(x).unwrap(),
                AlgebraicValue::U8(x) => encoder.encode_field(&(*x as i16)).unwrap(),
                AlgebraicValue::I16(x) => encoder.encode_field(x).unwrap(),
                AlgebraicValue::U16(x) => encoder.encode_field(&(*x as u32)).unwrap(),
                AlgebraicValue::I32(x) => encoder.encode_field(x).unwrap(),
                AlgebraicValue::U32(x) => encoder.encode_field(x).unwrap(),
                AlgebraicValue::I64(x) => encoder.encode_field(x).unwrap(),
                AlgebraicValue::U64(x) => encoder.encode_field(&(*x as u32)).unwrap(),
                AlgebraicValue::I128(x) => encoder.encode_field(&(format!("{x:?}"))).unwrap(),
                AlgebraicValue::U128(x) => encoder.encode_field(&(format!("{x:?}"))).unwrap(),
                AlgebraicValue::I256(x) => encoder.encode_field(&(x.to_string())).unwrap(),
                AlgebraicValue::U256(x) => encoder.encode_field(&(x.to_string())).unwrap(),
                AlgebraicValue::F32(x) => encoder.encode_field(&x.into_inner()).unwrap(),
                AlgebraicValue::F64(x) => encoder.encode_field(&x.into_inner()).unwrap(),
                AlgebraicValue::String(x) => encoder.encode_field(&x.as_ref()).unwrap(),
                AlgebraicValue::Array(x) => match x {
                    ArrayValue::Bool(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::I8(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::U8(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::I16(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::I32(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::U32(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::I64(x) => {
                        encoder.encode_field(&x.as_ref()).unwrap();
                    }
                    ArrayValue::F32(x) => {
                        let x = x.iter().map(|x| x.into_inner()).collect::<Vec<_>>();
                        encoder.encode_field(&x).unwrap();
                    }
                    ArrayValue::F64(x) => {
                        let x = x.iter().map(|x| x.into_inner()).collect::<Vec<_>>();
                        encoder.encode_field(&x).unwrap();
                    }
                    ArrayValue::String(x) => {
                        let x = x.iter().map(|x| x.as_ref()).collect::<Vec<_>>();
                        encoder.encode_field(&x).unwrap();
                    }
                    _ => encoder.encode_field(&value.to_satn()).unwrap(),
                },
                x => encoder.encode_field(&x.to_satn()).unwrap(),
            }
        }
        results.push(encoder.finish());
    }
    stream::iter(results)
}

fn row_desc_from_stmt(stmt: &SqlStmtResult<ProductValue>, format: &Format) -> Vec<FieldInfo> {
    let mut field_info = Vec::with_capacity(stmt.schema.elements.len());
    for (idx, ty) in stmt.schema.elements.iter().enumerate() {
        let field_name = ty.name.clone().map(Into::into).unwrap_or_else(|| format!("col {idx}"));
        let field_type = type_of(&ty.algebraic_type);
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

enum PgResult<T> {
    Ok(T),
    Fail(Box<ErrorInfo>),
    Err(PgWireError),
}

struct ResponseWrapper<T>(T);
impl<T> IntoResponse for ResponseWrapper<T> {
    fn into_response(self) -> axum::response::Response {
        unreachable!("Blank impl to satisfy IntoResponse")
    }
}

async fn response<T>(res: axum::response::Result<T>, database: &str) -> PgResult<T> {
    match res.map(ResponseWrapper) {
        Ok(sql) => PgResult::Ok(sql.0),
        err => {
            let res = err.into_response();
            if res.status() == StatusCode::NOT_FOUND {
                return PgResult::Fail(
                    ErrorInfo::new(
                        "FATAL".to_string(),
                        "3D000".to_string(),
                        format!("database \"{database}\" does not exist"),
                    )
                    .into(),
                );
            }
            let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let err = String::from_utf8_lossy(&bytes);
            PgResult::Err(PgError::Sql(format!("{err}")).into())
        }
    }
}

async fn decode_token(ctx: &StandaloneEnv, token: &str) -> Result<SpacetimeAuth, ErrorInfo> {
    match validate_token(ctx, token).await {
        Ok(claims) => Ok(SpacetimeAuth::from_claims(ctx, claims).unwrap()),
        Err(err) => Err(ErrorInfo::new("FATAL".to_owned(), "28P01".to_owned(), err.to_string())),
    }
}

struct PgSpacetimeDB {
    ctx: Arc<StandaloneEnv>,
    cached: Mutex<Metadata>,
    parameter_provider: DefaultServerParameterProvider,
}

impl PgSpacetimeDB {
    async fn exe_sql<'a>(&self, query: String) -> PgWireResult<Vec<Response<'a>>> {
        let params = self.cached.lock().await;
        let db = SqlParams {
            name_or_identity: database::NameOrIdentity::Name(DatabaseName(params.database.clone())),
        };

        let sql = match response(
            database::sql_direct(
                self.ctx.clone(),
                db,
                SqlQueryParams {},
                params.auth.clone(),
                query.to_string(),
            )
            .await,
            &params.database,
        )
        .await
        {
            PgResult::Ok(sql) => sql,
            PgResult::Fail(res) => {
                return Ok(vec![Response::Error(res)]);
            }
            PgResult::Err(err) => {
                return Err(err);
            }
        };

        let mut result = Vec::with_capacity(sql.len());
        for sql_result in sql {
            let header = Arc::new(row_desc_from_stmt(&sql_result, &Format::UnifiedText));

            let tag = Tag::new(&stats(&sql_result));

            if sql_result.rows.is_empty() {
                result.push(Response::EmptyQuery);
            } else {
                let rows = to_rows(sql_result, header.clone());
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
impl StartupHandler for PgSpacetimeDB {
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

                log::debug!("Login info: {:?}", login_info);

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
                let application_name = param("application_name")?;
                let pwd = pwd.into_password()?;

                log::info!("Connecting to database: {user}@{database}, by {application_name}",);

                let name = database::NameOrIdentity::Name(DatabaseName(database.clone()));
                match response(name.resolve(&self.ctx).await, &database).await {
                    PgResult::Ok(identity) => identity,
                    PgResult::Fail(res) => {
                        return close_client(client, *res).await;
                    }
                    PgResult::Err(err) => {
                        return Err(err);
                    }
                };

                let auth = match decode_token(&self.ctx, &pwd.password).await {
                    Ok(auth) => auth,
                    Err(err) => {
                        return close_client(client, err).await;
                    }
                };

                log::info!(
                    "Connected to database: {user}@{database} using identity `{}`",
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
impl SimpleQueryHandler for PgSpacetimeDB {
    async fn do_query<'a, C>(&self, _client: &mut C, query: &'a str) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        self.exe_sql(query.to_string()).await
    }
}

#[derive(Clone)]
struct PgSpacetimeDBFactory {
    handler: Arc<PgSpacetimeDB>,
}

impl PgWireServerHandlers for PgSpacetimeDBFactory {
    type StartupHandler = PgSpacetimeDB;
    type SimpleQueryHandler = PgSpacetimeDB;
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

fn setup_tls(ctx: &StandaloneEnv) -> Result<TlsAcceptor, io::Error> {
    let private = ctx.jwt_auth_provider().private_key_bytes();
    let private: PrivateKeyDer = PrivateKeyDer::from_pem_slice(private).unwrap();

    let keypair = KeyPair::from_der_and_sign_algo(&private, &rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|err| io::Error::new(ErrorKind::InvalidInput, err))?;

    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .unwrap();
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "localhost");
    let cert = params.self_signed(&keypair).unwrap();
    let cert_der = cert.der().clone();

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], private)
        .map_err(|err| io::Error::new(ErrorKind::InvalidInput, err))?;

    config.alpn_protocols = vec![b"postgresql".to_vec(), b"spacetime".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub async fn start_pg(mut shutdown: watch::Receiver<()>, ctx: Arc<StandaloneEnv>) {
    let mut parameter_provider = DefaultServerParameterProvider::default();
    parameter_provider.server_version = format!("spacetime {}", spacetimedb_lib_version());

    let tls_acceptor = Arc::new(setup_tls(&ctx).unwrap());

    let auth = SpacetimeAuth::alloc(&ctx).await.unwrap();
    let factory = PgSpacetimeDBFactory {
        handler: Arc::new(PgSpacetimeDB {
            ctx,
            cached: Mutex::new(Metadata {
                database: "".to_string(),
                auth,
            }),
            parameter_provider,
        }),
    };

    let server_addr = "127.0.0.1:5433";
    let tcp = TcpListener::bind(server_addr).await.unwrap();

    log::debug!(
        "Starting SpacetimeDB PG Protocol listening on {}",
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
                                log::error!("Error processing socket: {:?}", err);
                            })
                        });
                    }
                    Err(e) => {
                       log::error!("Accept error: {e}");
                    }
                }
            }
            _ = shutdown.changed() => {
                log::info!("Shutting down PostgreSQL server.");
                break;
            }
        }
    }
}
