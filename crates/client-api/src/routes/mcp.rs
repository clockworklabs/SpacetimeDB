use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::{ErrorResponse, IntoResponse, Response};
use axum::{Extension, Json};
use http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use spacetimedb::auth::identity::ConnectionAuthCtx;
use spacetimedb::host::{FunctionArgs, ReducerOutcome};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::Database;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::sats;

use super::database::{
    client_connected_error_to_response, client_disconnected_error_to_response, find_leader_and_database,
    find_module_and_database, map_reducer_error, sql_direct, SqlParams, SqlQueryParams,
};
use crate::auth::SpacetimeAuth;
use crate::routes::subscribe::generate_random_connection_id;
use crate::util::NameOrIdentity;
use crate::{log_and_500, Authorization, ControlStateDelegate, NodeDelegate};

const PROTOCOL_VERSION: &str = "2025-06-18";

const JSONRPC_VERSION: &str = "2.0";

const INVALID_REQUEST: i64 = -32600;

const METHOD_NOT_FOUND: i64 = -32601;

const INVALID_PARAMS: i64 = -32602;

const MODULE_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

type RpcError = (i64, String);

#[derive(Deserialize)]
pub struct McpParams {
    name_or_identity: NameOrIdentity,
}

/// handle MCP JSON-RPC request for the database named in the URL
pub async fn mcp<S>(
    State(ctx): State<S>,
    Path(McpParams { name_or_identity }): Path<McpParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    Json(request): Json<Value>,
) -> axum::response::Result<Response>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    handle_mcp(&ctx, Some(name_or_identity), auth, request).await
}

pub async fn mcp_root<S>(
    State(ctx): State<S>,
    Extension(auth): Extension<SpacetimeAuth>,
    Json(request): Json<Value>,
) -> axum::response::Result<Response>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    handle_mcp(&ctx, None, auth, request).await
}

async fn handle_mcp<S>(
    ctx: &S,
    scope: Option<NameOrIdentity>,
    auth: SpacetimeAuth,
    request: Value,
) -> axum::response::Result<Response>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    // a notification has no id, so it gets no response
    let Some(id) = request.get("id").cloned() else {
        return Ok(StatusCode::ACCEPTED.into_response());
    };

    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Ok(Json(jsonrpc_error(&id, INVALID_REQUEST, "invalid request: missing method")).into_response());
    };

    let host_wide = scope.is_none();
    let body = match method {
        "initialize" => jsonrpc_result(&id, initialize_result(host_wide)),
        // protocol ping, distinct from the ping tool
        "ping" => jsonrpc_result(&id, json!({})),
        "tools/list" => jsonrpc_result(&id, tools_list(host_wide)),
        "tools/call" => match tools_call(ctx, scope, auth, request.get("params")).await {
            Ok(result) => jsonrpc_result(&id, result),
            Err((code, message)) => jsonrpc_error(&id, code, message),
        },
        other => jsonrpc_error(&id, METHOD_NOT_FOUND, format!("method not found: {other}")),
    };
    Ok(Json(body).into_response())
}

fn jsonrpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "result": result })
}

fn jsonrpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "error": { "code": code, "message": message.into() } })
}

fn initialize_result(host_wide: bool) -> Value {
    let instructions = if host_wide {
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
    };
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "spacetimedb", "version": env!("CARGO_PKG_VERSION") },
        "instructions": instructions,
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
    scope: Option<NameOrIdentity>,
    auth: SpacetimeAuth,
    params: Option<&Value>,
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
        "get_schema" => tool_get_schema(ctx, target_database(&scope, arguments)?).await,
        "sql" => {
            let target = target_database(&scope, arguments)?;
            let Some(sql) = arguments.and_then(|a| a.get("sql")).and_then(Value::as_str) else {
                return Err((INVALID_PARAMS, "sql argument must be a string".to_owned()));
            };
            let confirmed = arguments.and_then(|a| a.get("confirmed")).and_then(Value::as_bool);
            tool_sql(ctx, target, auth, sql.to_owned(), confirmed).await
        }
        "call" => {
            let target = target_database(&scope, arguments)?;
            let Some(reducer) = arguments.and_then(|a| a.get("reducer")).and_then(Value::as_str) else {
                return Err((INVALID_PARAMS, "reducer argument must be a string".to_owned()));
            };
            let args_json = reducer_args_json(arguments)?;
            tool_call_reducer(ctx, target, auth, reducer.to_owned(), args_json).await
        }
        other => return Err((INVALID_PARAMS, format!("unknown tool: {other}"))),
    };

    Ok(match outcome {
        Ok(text) => json!({ "content": [ { "type": "text", "text": text } ], "isError": false }),
        Err(err) => execution_error_to_tool_result(err).await,
    })
}

fn target_database(scope: &Option<NameOrIdentity>, arguments: Option<&Value>) -> Result<NameOrIdentity, RpcError> {
    if let Some(name_or_identity) = scope {
        return Ok(name_or_identity.clone());
    }
    let Some(database) = arguments.and_then(|a| a.get("database")).and_then(Value::as_str) else {
        return Err((INVALID_PARAMS, "database argument must be a string".to_owned()));
    };
    serde_json::from_value(Value::String(database.to_owned()))
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

async fn tool_get_schema<S>(ctx: &S, name_or_identity: NameOrIdentity) -> axum::response::Result<String>
where
    S: ControlStateDelegate + NodeDelegate,
{
    let (leader, _) = find_leader_and_database(ctx, name_or_identity).await?;
    let module = leader.wait_for_module(MODULE_WAIT_TIMEOUT).await.map_err(log_and_500)?;
    let raw = RawModuleDefV9::from(module.info.module_def.as_ref().clone());
    let json = serde_json::to_string(&sats::serde::SerdeWrapper(raw)).map_err(log_and_500)?;
    Ok(json)
}

async fn tool_sql<S>(
    ctx: &S,
    name_or_identity: NameOrIdentity,
    auth: SpacetimeAuth,
    sql: String,
    confirmed: Option<bool>,
) -> axum::response::Result<String>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    let caller_identity = auth.claims.identity;
    let caller_auth: ConnectionAuthCtx = auth.into();
    let rows = sql_direct(
        ctx.clone(),
        SqlParams { name_or_identity },
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
    name_or_identity: NameOrIdentity,
    auth: SpacetimeAuth,
    reducer: String,
    args_json: String,
) -> axum::response::Result<String>
where
    S: ControlStateDelegate + NodeDelegate + Authorization + Clone + 'static,
{
    let caller_identity = auth.claims.identity;
    let caller_auth: ConnectionAuthCtx = auth.into();
    let (module, _) = find_module_and_database(ctx, name_or_identity).await?;

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
            assert_eq!(info["protocolVersion"], PROTOCOL_VERSION);
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

    #[test]
    fn target_database_prefers_the_url_scope_then_the_argument() {
        let scoped: NameOrIdentity = serde_json::from_value(json!("mydb")).unwrap();

        let target = target_database(&Some(scoped), Some(&json!({ "database": "other" }))).unwrap();
        assert_eq!(target.to_string(), "mydb");

        let target = target_database(&None, Some(&json!({ "database": "mydb" }))).unwrap();
        assert_eq!(target.to_string(), "mydb");

        let target = target_database(&None, Some(&json!({ "database": "0".repeat(64) }))).unwrap();
        assert!(matches!(target, NameOrIdentity::Identity(_)));

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
}
