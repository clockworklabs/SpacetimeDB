use std::time::Duration;

use axum::extract::State;
use axum::response::{ErrorResponse, IntoResponse, Response};
use axum::{Extension, Json};
use base64::{engine::general_purpose, Engine};
use http::{HeaderMap, StatusCode};
use serde_json::{json, Value};
use spacetimedb::auth::identity::ConnectionAuthCtx;
use spacetimedb::host::{FunctionArgs, ReducerOutcome};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::Database;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::sats;

use super::database::{
    client_connected_error_to_response, client_disconnected_error_to_response, count_response_egress,
    find_database_leader, find_database_module, find_database_or_404, map_reducer_error, sql_direct, ResolvedDatabase,
    SqlQueryParams,
};
use crate::auth::SpacetimeAuth;
use crate::routes::subscribe::generate_random_connection_id;
use crate::util::NameOrIdentity;
use crate::{log_and_500, Authorization, ControlStateDelegate, NodeDelegate};

/// stateless revision
const PROTOCOL_VERSION: &str = "2026-07-28";

/// still answered so existing clients keep working
const LEGACY_PROTOCOL_VERSION: &str = "2025-06-18";

const SUPPORTED_VERSIONS: [&str; 2] = [PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSION];

const LEGACY_VERSIONS: [&str; 5] = [
    LEGACY_PROTOCOL_VERSION,
    "2025-11-25",
    "2025-03-26",
    "2024-11-05",
    "2024-10-07",
];

const JSONRPC_VERSION: &str = "2.0";

const INVALID_REQUEST: i64 = -32600;

const METHOD_NOT_FOUND: i64 = -32601;

const INVALID_PARAMS: i64 = -32602;

const HEADER_MISMATCH: i64 = -32020;

const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";

const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";

const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

const METHOD_HEADER: &str = "mcp-method";

const NAME_HEADER: &str = "mcp-name";

const LIST_CACHE_TTL_MS: u64 = 300_000;

const MODULE_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

type RpcError = (i64, String);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Era {
    /// 2026-07-28
    Stateless,
    /// any version in LEGACY_VERSIONS or no version header
    Handshake,
}

fn protocol_era(headers: &HeaderMap) -> Result<Era, Option<String>> {
    if headers.get(PROTOCOL_VERSION_HEADER).is_none() {
        return Ok(Era::Handshake);
    }

    let Some(requested) = single_header(headers, PROTOCOL_VERSION_HEADER) else {
        return Err(None);
    };

    if requested == PROTOCOL_VERSION {
        Ok(Era::Stateless)
    } else if LEGACY_VERSIONS.contains(&requested) {
        Ok(Era::Handshake)
    } else {
        Err(Some(requested.to_owned()))
    }
}

fn single_header<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    let mut values = headers.get_all(name).into_iter();
    match (values.next(), values.next()) {
        (Some(value), None) => value.to_str().ok(),
        _ => None,
    }
}

fn decode_header_value(value: &str) -> Option<String> {
    let Some(encoded) = value.strip_prefix("=?base64?").and_then(|rest| rest.strip_suffix("?=")) else {
        return Some(value.to_owned());
    };
    let bytes = general_purpose::STANDARD.decode(encoded).ok()?;

    String::from_utf8(bytes).ok()
}

fn validate_stateless_request(headers: &HeaderMap, request: &Value, method: &str) -> Result<(), RpcError> {
    let header = |name: &str| single_header(headers, name);
    let params = request.get("params");
    let meta = params.and_then(|params| params.get("_meta"));

    match header(METHOD_HEADER) {
        None => return Err((HEADER_MISMATCH, format!("missing or malformed {METHOD_HEADER} header"))),
        Some(value) if value != method => {
            return Err((
                HEADER_MISMATCH,
                format!("{METHOD_HEADER} is '{value}' but the body calls '{method}'"),
            ))
        }
        Some(_) => {}
    }

    let Some(version) = meta
        .and_then(|meta| meta.get(META_PROTOCOL_VERSION))
        .and_then(Value::as_str)
    else {
        return Err((
            INVALID_PARAMS,
            format!("missing {META_PROTOCOL_VERSION} in params._meta"),
        ));
    };

    if Some(version) != header(PROTOCOL_VERSION_HEADER) {
        return Err((
            HEADER_MISMATCH,
            format!("{PROTOCOL_VERSION_HEADER} does not match {META_PROTOCOL_VERSION}"),
        ));
    }

    if !meta
        .and_then(|meta| meta.get(META_CLIENT_CAPABILITIES))
        .is_some_and(Value::is_object)
    {
        return Err((
            INVALID_PARAMS,
            format!("missing {META_CLIENT_CAPABILITIES} in params._meta"),
        ));
    }

    if method == "tools/call" {
        let Some(name) = params.and_then(|params| params.get("name")).and_then(Value::as_str) else {
            return Err((INVALID_PARAMS, "missing tool name".to_owned()));
        };
        match header(NAME_HEADER).map(decode_header_value) {
            None => return Err((HEADER_MISMATCH, format!("missing or malformed {NAME_HEADER} header"))),
            Some(None) => {
                return Err((HEADER_MISMATCH, format!("{NAME_HEADER} did not decode")));
            }
            Some(Some(value)) if value != name => {
                return Err((
                    HEADER_MISMATCH,
                    format!("{NAME_HEADER} is '{value}' but the body calls '{name}'"),
                ));
            }
            Some(Some(_)) => {}
        }
    }

    Ok(())
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn unsupported_version(id: &Value, requested: &str) -> Response {
    let mut body = jsonrpc_error(
        id,
        UNSUPPORTED_PROTOCOL_VERSION,
        format!("unsupported protocol version: {requested}"),
    );
    body["error"]["data"] = json!({ "supported": SUPPORTED_VERSIONS, "requested": requested });

    json_response(StatusCode::BAD_REQUEST, body)
}

/// handle MCP JSON-RPC request for the database named in the URL
pub async fn mcp<S>(
    State(ctx): State<S>,
    Extension(ResolvedDatabase(database)): Extension<ResolvedDatabase>,
    Extension(auth): Extension<SpacetimeAuth>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> axum::response::Result<Response>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    // the middleware counts this route, so the addressed database is discarded
    let mut discarded = None;

    handle_mcp(&ctx, Some(database), auth, headers, request, &mut discarded).await
}

pub async fn mcp_root<S>(
    State(ctx): State<S>,
    Extension(auth): Extension<SpacetimeAuth>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> axum::response::Result<Response>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    let mut addressed = None;
    let response = handle_mcp(&ctx, None, auth, headers, request, &mut addressed).await?;

    // no path middleware can attribute this route, its database is named in the request body
    Ok(match addressed {
        Some(database_identity) => count_response_egress(database_identity, response),
        None => response,
    })
}

async fn handle_mcp<S>(
    ctx: &S,
    scope: Option<Database>,
    auth: SpacetimeAuth,
    headers: HeaderMap,
    request: Value,
    // set to the database a tool addressed, so mcp_root can attribute its egress
    addressed: &mut Option<Identity>,
) -> axum::response::Result<Response>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    if !request.is_object() {
        let body = jsonrpc_error(&Value::Null, INVALID_REQUEST, "invalid request: expected an object");
        return Ok(json_response(StatusCode::BAD_REQUEST, body));
    }

    // a notification has no id, so it gets no response
    let Some(id) = request.get("id").cloned() else {
        return Ok(StatusCode::ACCEPTED.into_response());
    };

    let era = match protocol_era(&headers) {
        Ok(era) => era,
        Err(None) => {
            let message = format!("malformed {PROTOCOL_VERSION_HEADER} header");
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                jsonrpc_error(&id, HEADER_MISMATCH, message),
            ));
        }
        Err(Some(requested)) => return Ok(unsupported_version(&id, &requested)),
    };

    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Ok(Json(jsonrpc_error(&id, INVALID_REQUEST, "invalid request: missing method")).into_response());
    };

    if era == Era::Stateless
        && let Err((code, message)) = validate_stateless_request(&headers, &request, method)
    {
        return Ok(json_response(
            StatusCode::BAD_REQUEST,
            jsonrpc_error(&id, code, message),
        ));
    }

    let host_wide = scope.is_none();
    let body = match (era, method) {
        (Era::Handshake, "initialize") => jsonrpc_result(&id, initialize_result(host_wide)),
        // protocol ping, distinct from the ping tool
        (Era::Handshake, "ping") => jsonrpc_result(&id, json!({})),
        (Era::Handshake, "tools/list") => jsonrpc_result(&id, tools_list(host_wide)),
        (_, "server/discover") => jsonrpc_result(&id, cacheable(discover_result(host_wide))),
        (Era::Stateless, "tools/list") => jsonrpc_result(&id, cacheable(tools_list(host_wide))),
        (_, "tools/call") => match tools_call(ctx, scope, auth, request.get("params"), addressed).await {
            Ok(result) if era == Era::Stateless => jsonrpc_result(&id, complete(result)),
            Ok(result) => jsonrpc_result(&id, result),
            Err((code, message)) => jsonrpc_error(&id, code, message),
        },
        (Era::Stateless, other) => {
            return Ok(json_response(
                StatusCode::NOT_FOUND,
                jsonrpc_error(&id, METHOD_NOT_FOUND, format!("method not found: {other}")),
            ))
        }
        (Era::Handshake, other) => jsonrpc_error(&id, METHOD_NOT_FOUND, format!("method not found: {other}")),
    };

    Ok(Json(body).into_response())
}

fn jsonrpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "result": result })
}

fn jsonrpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "error": { "code": code, "message": message.into() } })
}

fn server_info() -> Value {
    json!({ "name": "spacetimedb", "version": env!("CARGO_PKG_VERSION") })
}

fn complete(mut result: Value) -> Value {
    result["resultType"] = json!("complete");
    result["_meta"][META_SERVER_INFO] = server_info();
    result
}

fn cacheable(result: Value) -> Value {
    let mut result = complete(result);
    result["ttlMs"] = json!(LIST_CACHE_TTL_MS);
    result["cacheScope"] = json!("public");
    result
}

fn instructions(host_wide: bool) -> &'static str {
    if host_wide {
        "Tools for the SpacetimeDB databases you can reach on this host. Every data tool takes a \
         `database` argument, either a name or an identity. Use list_databases to see the ones you \
         own, get_schema to see a database's tables and reducers, sql to query data, and call to \
         invoke a reducer. Reducers are the usual way to write, and SQL writes require ownership. \
         Everything runs with your identity, exactly as over the HTTP API."
    } else {
        "Tools for the addressed SpacetimeDB database: ping, get_schema, sql, and call. \
         Use get_schema to see its tables and reducers, sql to query data, and call to \
         invoke a reducer. Reducers are the usual way to write, and SQL writes require \
         ownership. Everything runs with your identity, exactly as over the HTTP API."
    }
}

fn initialize_result(host_wide: bool) -> Value {
    json!({
        "protocolVersion": LEGACY_PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": server_info(),
        "instructions": instructions(host_wide),
    })
}

fn discover_result(host_wide: bool) -> Value {
    json!({
        "supportedVersions": SUPPORTED_VERSIONS,
        "capabilities": { "tools": {} },
        "instructions": instructions(host_wide),
    })
}

fn database_property() -> Value {
    json!({ "type": "string", "description": "The name or identity of the target database." })
}

fn input_schema(properties: Value, required: Vec<&str>) -> Value {
    let mut schema = json!({ "type": "object", "properties": properties });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema
}

fn tools_list(host_wide: bool) -> Value {
    let mut get_schema_properties = json!({});
    let mut sql_properties = json!({
        "sql": { "type": "string", "description": "The SQL statement to execute." },
        "confirmed": { "type": "boolean", "description": "Wait for the read to be durably confirmed." }
    });
    let mut call_properties = json!({
        "reducer": { "type": "string", "description": "The name of the reducer to invoke." },
        "args": { "type": "array", "description": "A JSON array of arguments to the reducer, in order. Omit or pass [] for none." }
    });
    let mut get_schema_required = vec![];
    let mut sql_required = vec!["sql"];
    let mut call_required = vec!["reducer"];

    if host_wide {
        get_schema_properties["database"] = database_property();
        sql_properties["database"] = database_property();
        call_properties["database"] = database_property();
        get_schema_required.push("database");
        sql_required.insert(0, "database");
        call_required.insert(0, "database");
    }

    let mut tools = vec![];
    if host_wide {
        tools.push(json!({
            "name": "list_databases",
            "description": "List the databases you own on this host, with their identity and names.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "title": "List databases", "readOnlyHint": true, "destructiveHint": false, "openWorldHint": false }
        }));
    }
    tools.push(json!({
        "name": "ping",
        "description": "Health check that echoes an optional message back.",
        "inputSchema": { "type": "object", "properties": { "message": { "type": "string" } } },
        "annotations": { "title": "Ping", "readOnlyHint": true, "destructiveHint": false, "openWorldHint": false }
    }));
    tools.push(json!({
        "name": "get_schema",
        "description": "Get the schema for the database as JSON, including its typespace, tables, and reducers.",
        "inputSchema": input_schema(get_schema_properties, get_schema_required),
        "annotations": { "title": "Get schema", "readOnlyHint": true, "destructiveHint": false, "openWorldHint": false }
    }));
    tools.push(json!({
        "name": "sql",
        "description": "Run a SQL query against the database and return the rows as JSON. \
                        Write queries require ownership of the database.",
        "inputSchema": input_schema(sql_properties, sql_required),
        "annotations": { "title": "Run SQL", "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false }
    }));
    tools.push(json!({
        "name": "call",
        "description": "Invoke a reducer with positional JSON arguments, for example [\"alice\"] or [42]. \
                        The reducer runs with your identity and is the standard way to write.",
        "inputSchema": input_schema(call_properties, call_required),
        "annotations": { "title": "Call reducer", "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false }
    }));

    json!({ "tools": tools })
}

async fn tools_call<S>(
    ctx: &S,
    scope: Option<Database>,
    auth: SpacetimeAuth,
    params: Option<&Value>,
    addressed: &mut Option<Identity>,
) -> Result<Value, RpcError>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    let Some(params) = params else {
        return Err((INVALID_PARAMS, "missing params".to_owned()));
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err((INVALID_PARAMS, "missing tool name".to_owned()));
    };
    let arguments = params.get("arguments");

    let outcome: axum::response::Result<String> = match name {
        "ping" => Ok(match arguments.and_then(|a| a.get("message")).and_then(Value::as_str) {
            Some(message) => format!("pong: {message}"),
            None => "pong".to_owned(),
        }),
        // offered only host-wide
        "list_databases" if scope.is_none() => tool_list_databases(ctx, auth.claims.identity).await,
        "get_schema" => tool_get_schema(ctx, target_database(&scope, arguments)?, addressed).await,
        "sql" => {
            let target = target_database(&scope, arguments)?;
            let Some(sql) = arguments.and_then(|a| a.get("sql")).and_then(Value::as_str) else {
                return Err((INVALID_PARAMS, "sql argument must be a string".to_owned()));
            };
            let confirmed = arguments.and_then(|a| a.get("confirmed")).and_then(Value::as_bool);
            tool_sql(ctx, target, auth, sql.to_owned(), confirmed, addressed).await
        }
        "call" => {
            let target = target_database(&scope, arguments)?;
            let Some(reducer) = arguments.and_then(|a| a.get("reducer")).and_then(Value::as_str) else {
                return Err((INVALID_PARAMS, "reducer argument must be a string".to_owned()));
            };
            let args_json = reducer_args_json(arguments)?;
            tool_call_reducer(ctx, target, auth, reducer.to_owned(), args_json, addressed).await
        }
        other => return Err((INVALID_PARAMS, format!("unknown tool: {other}"))),
    };

    Ok(match outcome {
        Ok(text) => json!({ "content": [ { "type": "text", "text": text } ], "isError": false }),
        Err(err) => execution_error_to_tool_result(err).await,
    })
}

enum Target {
    Resolved(Database),
    Named(NameOrIdentity),
}

impl Target {
    /// an ErrorResponse here becomes an in-band tool error, an RpcError would be a protocol error
    async fn resolve(
        self,
        ctx: &(impl ControlStateDelegate + ?Sized),
        addressed: &mut Option<Identity>,
    ) -> axum::response::Result<Database> {
        let database = match self {
            Target::Resolved(database) => database,
            Target::Named(name_or_identity) => find_database_or_404(ctx, name_or_identity).await?,
        };
        *addressed = Some(database.database_identity);

        Ok(database)
    }
}

fn target_database(scope: &Option<Database>, arguments: Option<&Value>) -> Result<Target, RpcError> {
    if let Some(database) = scope {
        return Ok(Target::Resolved(database.clone()));
    }
    let Some(database) = arguments.and_then(|a| a.get("database")).and_then(Value::as_str) else {
        return Err((INVALID_PARAMS, "database argument must be a string".to_owned()));
    };
    serde_json::from_value(Value::String(database.to_owned()))
        .map(Target::Named)
        .map_err(|e| (INVALID_PARAMS, format!("invalid database '{database}': {e}")))
}

fn reducer_args_json(arguments: Option<&Value>) -> Result<String, RpcError> {
    match arguments.and_then(|a| a.get("args")) {
        None | Some(Value::Null) => Ok("[]".to_owned()),
        Some(args @ Value::Array(_)) => Ok(args.to_string()),
        Some(_) => Err((INVALID_PARAMS, "args must be a JSON array".to_owned())),
    }
}

async fn execution_error_to_tool_result(err: ErrorResponse) -> Value {
    let response = Err::<(), ErrorResponse>(err).into_response();
    let status = response.status();
    let text = axum::body::to_bytes(response.into_body(), MAX_ERROR_BODY_BYTES)
        .await
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
        .filter(|body| !body.is_empty())
        .unwrap_or_else(|| format!("request failed with HTTP {status}"));
    json!({ "content": [ { "type": "text", "text": text } ], "isError": true })
}

fn owned_by(databases: Vec<Database>, caller: Identity) -> Vec<Database> {
    databases
        .into_iter()
        .filter(|database| database.owner_identity == caller)
        .collect()
}

/// list only caller own databases
async fn tool_list_databases<S>(ctx: &S, caller: Identity) -> axum::response::Result<String>
where
    S: ControlStateDelegate,
{
    let owned = owned_by(ctx.get_databases().await.map_err(log_and_500)?, caller);

    let mut databases = Vec::new();
    for database in owned {
        let names = ctx
            .reverse_lookup(&database.database_identity)
            .await
            .map_err(log_and_500)?;
        databases.push(json!({
            "identity": database.database_identity.to_hex().to_string(),
            "names": names.iter().map(ToString::to_string).collect::<Vec<_>>(),
        }));
    }
    serde_json::to_string(&json!({ "databases": databases })).map_err(log_and_500)
}

async fn tool_get_schema<S>(ctx: &S, target: Target, addressed: &mut Option<Identity>) -> axum::response::Result<String>
where
    S: ControlStateDelegate + NodeDelegate,
{
    let database = target.resolve(ctx, addressed).await?;
    let leader = find_database_leader(ctx, &database).await?;
    let module = leader.wait_for_module(MODULE_WAIT_TIMEOUT).await.map_err(log_and_500)?;
    let raw = RawModuleDefV9::from(module.info.module_def.as_ref().clone());
    let json = serde_json::to_string(&sats::serde::SerdeWrapper(raw)).map_err(log_and_500)?;
    Ok(json)
}

async fn tool_sql<S>(
    ctx: &S,
    target: Target,
    auth: SpacetimeAuth,
    sql: String,
    confirmed: Option<bool>,
    addressed: &mut Option<Identity>,
) -> axum::response::Result<String>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    let caller_identity = auth.claims.identity;
    let caller_auth: ConnectionAuthCtx = auth.into();
    let database = target.resolve(ctx, addressed).await?;
    let rows = sql_direct(
        ctx.clone(),
        database,
        SqlQueryParams { confirmed },
        caller_identity,
        caller_auth,
        sql,
    )
    .await?;
    let json = serde_json::to_string(&rows).map_err(log_and_500)?;
    Ok(json)
}

async fn tool_call_reducer<S>(
    ctx: &S,
    target: Target,
    auth: SpacetimeAuth,
    reducer: String,
    args_json: String,
    addressed: &mut Option<Identity>,
) -> axum::response::Result<String>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    let caller_identity = auth.claims.identity;
    let caller_auth: ConnectionAuthCtx = auth.into();
    let database = target.resolve(ctx, addressed).await?;
    let module = find_database_module(ctx, &database).await?;

    let connection_id = generate_random_connection_id();
    module
        .call_identity_connected(caller_auth, connection_id)
        .await
        .map_err(client_connected_error_to_response)?;
    let outcome = module
        .call_reducer(
            caller_identity,
            Some(connection_id),
            None,
            None,
            None,
            &reducer,
            FunctionArgs::Json(args_json.into()),
        )
        .await;
    module
        .call_identity_disconnected(caller_identity, connection_id)
        .await
        .map_err(client_disconnected_error_to_response)?;

    let result = outcome.map_err(|e| map_reducer_error(e, &reducer))?;
    reducer_outcome_text(&reducer, result.outcome)
}

fn reducer_outcome_text(reducer: &str, outcome: ReducerOutcome) -> axum::response::Result<String> {
    let failed = outcome.is_err();
    let text = match outcome {
        ReducerOutcome::Committed => format!("reducer '{reducer}' committed"),
        ReducerOutcome::Failed(err) => format!("reducer '{reducer}' failed: {err}"),
        ReducerOutcome::BudgetExceeded => format!("reducer '{reducer}' exceeded the energy budget"),
    };
    if failed {
        Err((StatusCode::INTERNAL_SERVER_ERROR, text).into())
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_tools_and_identity() {
        for host_wide in [false, true] {
            let info = initialize_result(host_wide);
            assert_eq!(info["serverInfo"]["name"], "spacetimedb");
            assert_eq!(info["protocolVersion"], LEGACY_PROTOCOL_VERSION);
            assert!(info["capabilities"]["tools"].is_object());
            assert!(info["instructions"].as_str().unwrap().contains("SpacetimeDB"));
        }

        let host_wide = initialize_result(true);
        assert!(host_wide["instructions"].as_str().unwrap().contains("database"));
    }

    #[test]
    fn tools_list_exposes_the_expected_tools() {
        let listed = tools_list(false);
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["ping", "get_schema", "sql", "call"]);

        let sql_tool = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "sql")
            .unwrap();
        assert_eq!(sql_tool["inputSchema"]["required"], json!(["sql"]));

        let call_tool = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "call")
            .unwrap();
        assert_eq!(call_tool["inputSchema"]["required"], json!(["reducer"]));

        assert_eq!(sql_tool["annotations"]["readOnlyHint"], false);
        assert_eq!(call_tool["annotations"]["readOnlyHint"], false);

        // every tool carries human readable title so MCP clients can label it
        let every_tool = [tools_list(false), tools_list(true)];
        for tool in every_tool.iter().flat_map(|listed| listed["tools"].as_array().unwrap()) {
            let annotations = &tool["annotations"];
            assert!(
                annotations["title"].as_str().is_some(),
                "tool {} is missing annotations.title",
                tool["name"]
            );
            for hint in ["readOnlyHint", "destructiveHint", "openWorldHint"] {
                assert!(
                    annotations[hint].is_boolean(),
                    "tool {} is missing annotations.{hint}",
                    tool["name"]
                );
            }
            assert_eq!(
                annotations["openWorldHint"], false,
                "tool {} must set openWorldHint to false",
                tool["name"]
            );
            // read only tool cannot be destructive
            if annotations["readOnlyHint"] == true {
                assert_eq!(
                    annotations["destructiveHint"], false,
                    "read-only tool {} must set destructiveHint to false",
                    tool["name"]
                );
            }
        }
    }

    #[test]
    fn host_wide_tools_take_a_database_argument() {
        let listed = tools_list(true);
        let tools = listed["tools"].as_array().unwrap().clone();
        let names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["list_databases", "ping", "get_schema", "sql", "call"]);

        let tool = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap().clone();

        for name in ["get_schema", "sql", "call"] {
            let schema = tool(name)["inputSchema"].clone();
            assert!(
                schema["properties"]["database"].is_object(),
                "{name} is missing the database property"
            );
            let required = schema["required"].as_array().unwrap();
            assert!(
                required.iter().any(|arg| arg == "database"),
                "{name} does not require database"
            );
        }
        assert_eq!(tool("sql")["inputSchema"]["required"], json!(["database", "sql"]));
        assert_eq!(tool("call")["inputSchema"]["required"], json!(["database", "reducer"]));

        assert!(tool("ping")["inputSchema"]["properties"]["database"].is_null());
        assert!(tool("list_databases")["inputSchema"]["properties"]["database"].is_null());

        for tool in &tools {
            assert!(tool["annotations"]["title"].as_str().is_some());
        }
    }

    #[test]
    fn scoped_tools_omit_the_database_argument_and_list_tool() {
        let listed = tools_list(false);
        for tool in listed["tools"].as_array().unwrap() {
            assert_ne!(tool["name"], "list_databases");
            assert!(tool["inputSchema"]["properties"]["database"].is_null());
        }

        let get_schema = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "get_schema")
            .unwrap()
            .clone();
        assert!(get_schema["inputSchema"]["required"].is_null());
    }

    fn test_database() -> Database {
        use spacetimedb::messages::control_db::HostType;
        use spacetimedb_lib::Hash;

        Database {
            id: 1,
            database_identity: Identity::from_byte_array([7; 32]),
            owner_identity: Identity::from_byte_array([8; 32]),
            host_type: HostType::Wasm,
            initial_program: Hash::ZERO,
            bootstrap_generation: 0,
        }
    }

    #[test]
    fn target_database_prefers_the_url_scope_then_the_argument() {
        let scoped = test_database();

        let scope = Some(scoped.clone());
        let target = target_database(&scope, Some(&json!({ "database": "other" }))).unwrap();
        match target {
            Target::Resolved(database) => assert_eq!(database.database_identity, scoped.database_identity),
            Target::Named(_) => panic!("expected the database the middleware already resolved"),
        }

        let target = target_database(&None, Some(&json!({ "database": "mydb" }))).unwrap();
        match target {
            Target::Named(NameOrIdentity::Name(name)) => assert_eq!(name.as_ref(), "mydb"),
            _ => panic!("expected a name"),
        }

        let target = target_database(&None, Some(&json!({ "database": "0".repeat(64) }))).unwrap();
        assert!(matches!(target, Target::Named(NameOrIdentity::Identity(_))));

        assert!(target_database(&None, None).is_err());
        assert!(target_database(&None, Some(&json!({}))).is_err());
        assert!(target_database(&None, Some(&json!({ "database": 42 }))).is_err());
    }

    #[test]
    fn reducer_failures_report_as_errors() {
        assert!(reducer_outcome_text("add", ReducerOutcome::Committed).is_ok());
        assert!(reducer_outcome_text("add", ReducerOutcome::Failed(Box::new("boom".into()))).is_err());
        assert!(reducer_outcome_text("add", ReducerOutcome::BudgetExceeded).is_err());

        let committed = reducer_outcome_text("add", ReducerOutcome::Committed).unwrap();
        assert!(committed.contains("add") && committed.contains("committed"));
    }

    #[tokio::test]
    async fn failed_reducer_surfaces_in_band_with_message() {
        let err = reducer_outcome_text("add", ReducerOutcome::Failed(Box::new("boom".into()))).unwrap_err();
        let result = execution_error_to_tool_result(err).await;
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn execution_errors_become_in_band_tool_results() {
        let err: ErrorResponse = (StatusCode::NOT_FOUND, "no such database").into();
        let result = execution_error_to_tool_result(err).await;
        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["type"], "text");
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no such database"));
    }

    #[test]
    fn jsonrpc_envelopes_are_well_formed() {
        let id = json!(7);

        let ok = jsonrpc_result(&id, json!({ "x": 1 }));
        assert_eq!(ok["jsonrpc"], "2.0");
        assert_eq!(ok["id"], 7);
        assert_eq!(ok["result"]["x"], 1);

        let err = jsonrpc_error(&id, -32602, "bad");
        assert_eq!(err["jsonrpc"], "2.0");
        assert_eq!(err["id"], 7);
        assert_eq!(err["error"]["code"], -32602);
        assert_eq!(err["error"]["message"], "bad");
    }

    #[test]
    fn reducer_args_json_normalizes_and_rejects() {
        assert_eq!(reducer_args_json(None).unwrap(), "[]");
        assert_eq!(reducer_args_json(Some(&json!({}))).unwrap(), "[]");
        assert_eq!(reducer_args_json(Some(&json!({ "args": null }))).unwrap(), "[]");
        assert_eq!(
            reducer_args_json(Some(&json!({ "args": ["alice", 42] }))).unwrap(),
            "[\"alice\",42]"
        );
        assert!(reducer_args_json(Some(&json!({ "args": "nope" }))).is_err());
        assert!(reducer_args_json(Some(&json!({ "args": {} }))).is_err());
    }

    #[test]
    fn list_databases_shows_only_the_callers_own() {
        use spacetimedb::messages::control_db::HostType;
        use spacetimedb_lib::Hash;

        let caller = Identity::from_hex("11".repeat(32)).unwrap();
        let other = Identity::from_hex("22".repeat(32)).unwrap();
        let database = |owner, id| Database {
            id,
            database_identity: Default::default(),
            owner_identity: owner,
            host_type: HostType::Wasm,
            initial_program: Hash::ZERO,
            bootstrap_generation: 0,
        };

        let owned = owned_by(
            vec![database(caller, 1), database(other, 2), database(caller, 3)],
            caller,
        );
        let ids: Vec<u64> = owned.iter().map(|database| database.id).collect();
        assert_eq!(ids, [1, 3], "a listing must never include another owner's database");

        assert!(owned_by(vec![database(other, 2)], caller).is_empty());
    }

    fn stateless_headers(method: &str, name: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(PROTOCOL_VERSION_HEADER, PROTOCOL_VERSION.parse().unwrap());
        headers.insert(METHOD_HEADER, method.parse().unwrap());
        if let Some(name) = name {
            headers.insert(NAME_HEADER, name.parse().unwrap());
        }
        headers
    }

    fn version_header(version: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(PROTOCOL_VERSION_HEADER, version.parse().unwrap());
        headers
    }

    fn request_meta(version: &str, capabilities: bool) -> Value {
        let mut meta = serde_json::Map::new();
        meta.insert(META_PROTOCOL_VERSION.to_owned(), json!(version));
        if capabilities {
            meta.insert(META_CLIENT_CAPABILITIES.to_owned(), json!({}));
        }
        Value::Object(meta)
    }

    fn stateless_request(method: &str, mut params: Value) -> Value {
        params["_meta"] = request_meta(PROTOCOL_VERSION, true);
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    #[test]
    fn the_version_header_picks_the_protocol_era() {
        assert_eq!(protocol_era(&version_header(PROTOCOL_VERSION)).unwrap(), Era::Stateless);

        // existing clients older than the header use the handshake
        assert_eq!(protocol_era(&HeaderMap::new()).unwrap(), Era::Handshake);

        for version in LEGACY_VERSIONS {
            assert_eq!(
                protocol_era(&version_header(version)).unwrap(),
                Era::Handshake,
                "{version} clients must keep working"
            );
        }

        let unsupported = protocol_era(&version_header("2099-01-01")).unwrap_err();
        assert_eq!(unsupported.as_deref(), Some("2099-01-01"));
    }

    #[test]
    fn a_malformed_version_header_is_rejected_not_demoted() {
        let mut duplicated = HeaderMap::new();
        duplicated.append(PROTOCOL_VERSION_HEADER, PROTOCOL_VERSION.parse().unwrap());
        duplicated.append(PROTOCOL_VERSION_HEADER, LEGACY_PROTOCOL_VERSION.parse().unwrap());
        assert_eq!(protocol_era(&duplicated).unwrap_err(), None);

        let mut unreadable = HeaderMap::new();
        unreadable.insert(PROTOCOL_VERSION_HEADER, http::HeaderValue::from_bytes(&[0xff]).unwrap());
        assert_eq!(
            protocol_era(&unreadable).unwrap_err(),
            None,
            "an unreadable value must not be served as the handshake era"
        );
    }

    #[test]
    fn a_duplicated_mirrored_header_is_rejected() {
        let request = stateless_request("tools/list", json!({}));
        let mut headers = stateless_headers("tools/list", None);
        headers.append(METHOD_HEADER, "tools/call".parse().unwrap());

        let (code, _) = validate_stateless_request(&headers, &request, "tools/list").unwrap_err();
        assert_eq!(code, HEADER_MISMATCH, "an intermediary could route on the second value");
    }

    #[test]
    fn completing_a_result_keeps_meta_it_already_set() {
        let result = complete(json!({ "_meta": { "vendor/key": 1 } }));

        assert_eq!(result["_meta"]["vendor/key"], 1, "an existing key must survive");
        assert_eq!(result["_meta"][META_SERVER_INFO]["name"], "spacetimedb");
    }

    #[tokio::test]
    async fn an_unsupported_version_reports_what_the_server_speaks() {
        let response = unsupported_version(&json!(1), "2099-01-01");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = axum::body::to_bytes(response.into_body(), MAX_ERROR_BODY_BYTES)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], UNSUPPORTED_PROTOCOL_VERSION);
        assert_eq!(body["error"]["data"]["requested"], "2099-01-01");
        assert_eq!(body["error"]["data"]["supported"], json!(SUPPORTED_VERSIONS));
    }

    #[test]
    fn stateless_requests_mirror_the_method_into_a_header() {
        let request = stateless_request("tools/list", json!({}));

        assert!(validate_stateless_request(&stateless_headers("tools/list", None), &request, "tools/list").is_ok());

        let (code, _) = validate_stateless_request(&HeaderMap::new(), &request, "tools/list").unwrap_err();
        assert_eq!(code, HEADER_MISMATCH, "the method header is required");

        let (code, _) =
            validate_stateless_request(&stateless_headers("tools/call", None), &request, "tools/list").unwrap_err();
        assert_eq!(code, HEADER_MISMATCH);
    }

    #[test]
    fn stateless_requests_carry_their_own_protocol_metadata() {
        let headers = stateless_headers("tools/list", None);

        let bare = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let (code, _) = validate_stateless_request(&headers, &bare, "tools/list").unwrap_err();
        assert_eq!(code, INVALID_PARAMS, "a request without params has no protocol version");

        let no_capabilities =
            json!({ "method": "tools/list", "params": { "_meta": request_meta(PROTOCOL_VERSION, false) } });
        let (code, _) = validate_stateless_request(&headers, &no_capabilities, "tools/list").unwrap_err();
        assert_eq!(
            code, INVALID_PARAMS,
            "client capabilities are required on every request"
        );

        let disagrees =
            json!({ "method": "tools/list", "params": { "_meta": request_meta(LEGACY_PROTOCOL_VERSION, true) } });
        let (code, _) = validate_stateless_request(&headers, &disagrees, "tools/list").unwrap_err();
        assert_eq!(
            code, HEADER_MISMATCH,
            "the header and the body must name the same version"
        );
    }

    #[test]
    fn tools_call_mirrors_the_tool_name_into_a_header() {
        let request = stateless_request("tools/call", json!({ "name": "sql", "arguments": {} }));
        let validate =
            |headers: HeaderMap| validate_stateless_request(&headers, &request, "tools/call").map_err(|(code, _)| code);

        assert!(validate(stateless_headers("tools/call", Some("sql"))).is_ok());

        assert_eq!(
            validate(stateless_headers("tools/call", None)).unwrap_err(),
            HEADER_MISMATCH,
            "the name header is required for tools/call"
        );
        assert_eq!(
            validate(stateless_headers("tools/call", Some("call"))).unwrap_err(),
            HEADER_MISMATCH,
            "a name that disagrees with the body must be rejected"
        );

        assert!(validate(stateless_headers("tools/call", Some("=?base64?c3Fs?="))).is_ok());
        assert_eq!(
            validate(stateless_headers("tools/call", Some("=?base64?not valid?="))).unwrap_err(),
            HEADER_MISMATCH
        );
    }

    #[test]
    fn header_values_decode_the_base64_sentinel() {
        assert_eq!(decode_header_value("sql").unwrap(), "sql");
        assert_eq!(decode_header_value("=?base64?c3Fs?=").unwrap(), "sql");
        // non ASCII names are sent base64 wrapped
        assert_eq!(
            decode_header_value("=?base64?SGVsbG8sIOS4lueVjA==?=").unwrap(),
            "Hello, 世界"
        );
        assert!(decode_header_value("=?base64?not valid?=").is_none());
    }

    #[test]
    fn stateless_results_state_completion_and_identity() {
        let result = complete(json!({ "content": [], "isError": false }));
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["_meta"][META_SERVER_INFO]["name"], "spacetimedb");
        assert_eq!(result["isError"], false, "decorating must not disturb the result");
    }

    #[test]
    fn listings_carry_cache_hints() {
        let listed = cacheable(tools_list(true));
        assert_eq!(listed["resultType"], "complete");
        assert_eq!(listed["cacheScope"], "public");
        assert!(listed["ttlMs"].as_u64().is_some_and(|ttl| ttl > 0));
        assert_eq!(listed["tools"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn discover_advertises_every_version_the_server_answers() {
        let discovered = cacheable(discover_result(true));
        assert_eq!(discovered["supportedVersions"], json!(SUPPORTED_VERSIONS));
        assert!(discovered["capabilities"]["tools"].is_object());
        assert!(discovered["instructions"].as_str().unwrap().contains("SpacetimeDB"));
        assert_eq!(discovered["resultType"], "complete");
        assert_eq!(discovered["_meta"][META_SERVER_INFO]["name"], "spacetimedb");
        assert_eq!(discovered["cacheScope"], "public");

        // every version advertised needs to be accepted
        for version in SUPPORTED_VERSIONS {
            assert!(
                protocol_era(&version_header(version)).is_ok(),
                "{version} is advertised but not accepted"
            );
        }
    }

    #[test]
    fn handshake_results_keep_the_shape_older_clients_expect() {
        let info = initialize_result(true);
        assert!(info["resultType"].is_null(), "a 2025-06-18 client has no resultType");
        assert!(info["_meta"].is_null());

        let listed = tools_list(true);
        assert!(listed["resultType"].is_null());
        assert!(listed["ttlMs"].is_null());
        assert!(listed["cacheScope"].is_null());
    }
}
