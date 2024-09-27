use crate::auth::{
    anon_auth_middleware, SpacetimeAuth, SpacetimeAuthHeader, SpacetimeEnergyUsed, SpacetimeExecutionDurationMicros,
    SpacetimeIdentity, SpacetimeIdentityToken,
};
use crate::routes::subscribe::generate_random_address;
use crate::util::{ByteStringBody, NameOrAddress};
use crate::{log_and_500, ControlStateDelegate, DatabaseDef, NodeDelegate};
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::response::{ErrorResponse, IntoResponse};
use axum::Extension;
use axum_extra::TypedHeader;
use futures::StreamExt;
use http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use spacetimedb::address::Address;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::ReducerCallError;
use spacetimedb::host::ReducerOutcome;
use spacetimedb::host::{DescribedEntityType, UpdateDatabaseResult};
use spacetimedb::host::{ModuleHost, ReducerArgs};
use spacetimedb::identity::Identity;
use spacetimedb::json::client_api::StmtResultJson;
use spacetimedb::messages::control_db::{Database, DatabaseInstance, HostType};
use spacetimedb::sql;
use spacetimedb::sql::execute::{ctx_sql, translate_col};
use spacetimedb_client_api_messages::name::{self, DnsLookupResponse, DomainName, PublishOp, PublishResult};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::address::AddressForUrl;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::sats::WithTypespace;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::ProductTypeElement;
use spacetimedb_schema::def::{ReducerDef, TableDef};

pub(crate) struct DomainParsingRejection;

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
    Extension(auth): Extension<SpacetimeAuth>,
    Path(CallParams {
        name_or_address,
        reducer,
    }): Path<CallParams>,
    Query(CallQueryParams { client_address }): Query<CallQueryParams>,
    ByteStringBody(body): ByteStringBody,
) -> axum::response::Result<impl IntoResponse> {
    let caller_identity = auth.identity;

    let args = ReducerArgs::Json(body);

    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address).await?.ok_or_else(|| {
        log::error!("Could not find database: {}", address.to_hex());
        (StatusCode::NOT_FOUND, "No such database.")
    })?;
    let identity = database.owner_identity;
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
        .ok_or((
            StatusCode::NOT_FOUND,
            "Database instance not scheduled to this node yet.",
        ))?;
    let instance_id = database_instance.id;
    let host = worker_ctx.host_controller();
    let module = host
        .get_or_launch_module_host(database, instance_id)
        .await
        .map_err(log_and_500)?;

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
        .call_reducer(caller_identity, Some(client_address), None, None, None, &reducer, args)
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
    auth: SpacetimeAuth,
    address: &Address,
) -> Result<DatabaseInformation, ErrorResponse> {
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

pub enum EntityDef<'a> {
    Reducer(&'a ReducerDef),
    Table(&'a TableDef),
}

impl<'a> EntityDef<'a> {
    fn described_entity_ty(&self) -> DescribedEntityType {
        match self {
            EntityDef::Reducer(_) => DescribedEntityType::Reducer,
            EntityDef::Table(_) => DescribedEntityType::Table,
        }
    }
    fn name(&self) -> &str {
        match self {
            EntityDef::Reducer(r) => &r.name[..],
            EntityDef::Table(t) => &t.name[..],
        }
    }
}

fn entity_description_json(description: WithTypespace<EntityDef>, expand: bool) -> Option<Value> {
    let typ = description.ty().described_entity_ty().as_str();
    let len = match description.ty() {
        EntityDef::Table(t) => description
            .resolve(t.product_type_ref)
            .ty()
            .as_product()?
            .elements
            .len(),
        EntityDef::Reducer(r) => r.params.elements.len(),
    };
    if expand {
        // TODO(noa): make this less hacky; needs coordination w/ spacetime-web
        let schema = match description.ty() {
            EntityDef::Table(table) => {
                json!(description
                    .with(&table.product_type_ref)
                    .resolve_refs()
                    .ok()?
                    .as_product()?)
            }
            EntityDef::Reducer(r) => json!({
                "name": &r.name[..],
                "elements": r.params.elements,
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
    Extension(auth): Extension<SpacetimeAuth>,
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
    let module = worker_ctx
        .host_controller()
        .get_or_launch_module_host(database, instance_id)
        .await
        .map_err(log_and_500)?;

    let entity_type = entity_type.as_str().parse().map_err(|()| {
        log::debug!("Request to describe unhandled entity type: {}", entity_type);
        (
            StatusCode::NOT_FOUND,
            format!("Invalid entity type for description: {}", entity_type),
        )
    })?;
    let description = get_entity(&module, &entity, entity_type)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("{entity_type} {entity:?} not found")))?;
    let description = WithTypespace::new(module.info().module_def.typespace(), &description);

    let expand = expand.unwrap_or(true);
    let response_json = json!({ entity: entity_description_json(description, expand) });

    Ok((
        StatusCode::OK,
        TypedHeader(SpacetimeIdentity(call_info.auth.identity)),
        TypedHeader(SpacetimeIdentityToken(call_info.auth.creds)),
        axum::Json(response_json),
    ))
}

fn get_catalog(host: &ModuleHost) -> impl Iterator<Item = EntityDef> {
    let module_def = &host.info().module_def;
    module_def
        .reducers()
        .map(EntityDef::Reducer)
        .chain(module_def.tables().map(EntityDef::Table))
}

fn get_entity<'a>(host: &'a ModuleHost, entity: &'_ str, entity_type: DescribedEntityType) -> Option<EntityDef<'a>> {
    match entity_type {
        DescribedEntityType::Table => host.info().module_def.table(entity).map(EntityDef::Table),
        DescribedEntityType::Reducer => host.info().module_def.reducer(entity).map(EntityDef::Reducer),
    }
}

#[derive(Deserialize)]
pub struct CatalogParams {
    name_or_address: NameOrAddress,
}
#[derive(Deserialize)]
pub struct CatalogQueryParams {
    expand: Option<bool>,
    #[serde(default)]
    module_def: bool,
}
pub async fn catalog<S>(
    State(worker_ctx): State<S>,
    Path(CatalogParams { name_or_address }): Path<CatalogParams>,
    Query(CatalogQueryParams { expand, module_def }): Query<CatalogQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    if module_def && expand.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "expand and module_def cannot both be specified",
        )
            .into());
    }
    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let call_info = extract_db_call_info(&worker_ctx, auth, &address).await?;

    let instance_id = call_info.database_instance.id;
    let host = worker_ctx.host_controller();
    let module = host
        .get_or_launch_module_host(database, instance_id)
        .await
        .map_err(log_and_500)?;

    let response_json = if module_def {
        let raw = RawModuleDefV9::from(module.info().module_def.clone());
        serde_json::to_value(SerializeWrapper::from_ref(&raw)).map_err(log_and_500)?
    } else {
        let expand = expand.unwrap_or(false);
        let response_catalog: HashMap<_, _> = get_catalog(&module)
            .map(|entity| {
                (
                    entity.name().to_string().into_boxed_str(),
                    entity_description_json(
                        WithTypespace::new(module.info().module_def.typespace(), &entity),
                        expand,
                    ),
                )
            })
            .collect();
        json!({
            "entities": response_catalog,
            "typespace": SerializeWrapper::from_ref(module.info().module_def.typespace()),
        })
    };

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
        "owner_identity": database.owner_identity,
        "host_type": host_type,
        "initial_program": database.initial_program,
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

pub async fn logs<S>(
    State(worker_ctx): State<S>,
    Path(LogsParams { name_or_address }): Path<LogsParams>,
    Query(LogsQuery { num_lines, follow }): Query<LogsQuery>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse>
where
    S: ControlStateDelegate + NodeDelegate,
{
    // You should not be able to read the logs from a database that you do not own
    // so, unless you are the owner, this will fail.

    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
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
        let module = host
            .get_or_launch_module_host(database, instance_id)
            .await
            .map_err(log_and_500)?;
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
    Extension(auth): Extension<SpacetimeAuth>,
    body: String,
) -> axum::response::Result<impl IntoResponse>
where
    S: NodeDelegate + ControlStateDelegate,
{
    // Anyone is authorized to execute SQL queries. The SQL engine will determine
    // which queries this identity is allowed to execute against the database.

    let address = name_or_address.resolve(&worker_ctx).await?.into();
    let database = worker_ctx_find_database(&worker_ctx, &address)
        .await?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    let auth = AuthCtx::new(database.owner_identity, auth.identity);
    log::debug!("auth: {auth:?}");
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
        .ok_or((
            StatusCode::NOT_FOUND,
            "Database instance not scheduled to this node yet.",
        ))?;
    let instance_id = database_instance.id;

    let host = worker_ctx.host_controller();
    let module_host = host
        .get_or_launch_module_host(database.clone(), instance_id)
        .await
        .map_err(log_and_500)?;
    let json = host
        .using_database(
            database,
            instance_id,
            move |db| -> axum::response::Result<_, (StatusCode, String)> {
                tracing::info!(sql = body);
                let results =
                    sql::execute::run(db, &body, auth, Some(&module_host.info().subscriptions)).map_err(|e| {
                        log::warn!("{}", e);
                        if let Some(auth_err) = e.get_auth_error() {
                            (StatusCode::UNAUTHORIZED, auth_err.to_string())
                        } else {
                            (StatusCode::BAD_REQUEST, e.to_string())
                        }
                    })?;

                let json = db.with_read_only(&ctx_sql(db), |tx| {
                    results
                        .into_iter()
                        .map(|result| {
                            let rows = result.data;
                            let schema = result
                                .head
                                .fields
                                .iter()
                                .map(|x| {
                                    let ty = x.algebraic_type.clone();
                                    let name = translate_col(tx, x.field);
                                    ProductTypeElement::new(ty, name)
                                })
                                .collect();
                            StmtResultJson { schema, rows }
                        })
                        .collect::<Vec<_>>()
                });

                Ok(json)
            },
        )
        .await
        .map_err(log_and_500)??;

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
    let domain = database_name.parse().map_err(|_| DomainParsingRejection)?;
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
    name_or_address: Option<NameOrAddress>,
}

impl PublishDatabaseQueryParams {
    pub fn name_or_address(&self) -> Option<&NameOrAddress> {
        self.name_or_address.as_ref()
    }
}

pub async fn publish<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams {}): Path<PublishDatabaseParams>,
    Query(query_params): Query<PublishDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    let PublishDatabaseQueryParams { name_or_address, clear } = query_params;

    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail.

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
            DatabaseDef {
                address: db_addr,
                program_bytes: body.into(),
                num_replicas: 1,
                host_type: HostType::Wasm,
            },
        )
        .await
        .map_err(log_and_500)?;

    if let Some(UpdateDatabaseResult::AutoMigrateError(errs)) = maybe_updated {
        return Err((StatusCode::BAD_REQUEST, format!("Database update rejected: {errs}")).into());
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
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
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
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let address = Address::from(address);

    let database = ctx
        .get_database_by_address(&address)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;

    if database.owner_identity != auth.identity {
        return Err((StatusCode::UNAUTHORIZED, "Identity does not own database.").into());
    }

    let domain = domain.parse().map_err(|_| DomainParsingRejection)?;
    let response = ctx
        .create_dns_record(&auth.identity, &domain, &address)
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
        .route("/reverse_dns/:database_address", get(reverse_dns::<S>))
        .route("/set_name", get(set_name::<S>))
        .route("/ping", get(ping::<S>))
        .route("/register_tld", get(register_tld::<S>))
        .route("/publish", post(publish::<S>).layer(DefaultBodyLimit::disable()))
        .route("/delete/:address", post(delete_database::<S>))
        .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
}

pub fn worker_routes<S>(ctx: S) -> axum::Router<S>
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
        .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
}
