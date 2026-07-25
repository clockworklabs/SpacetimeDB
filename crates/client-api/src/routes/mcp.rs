use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::{ErrorResponse, IntoResponse, Response};
use axum::{Extension, Json};
use http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use spacetimedb::auth::identity::ConnectionAuthCtx;
use spacetimedb::host::{FunctionArgs, ReducerOutcome};
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

/// handle MCP JSON-RPC request
pub async fn mcp<S>(
    State(ctx): State<S>,
    Path(McpParams { name_or_identity }): Path<McpParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    Json(request): Json<Value>,
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

    let body = match method {
        "initialize" => jsonrpc_result(&id, initialize_result()),
        // protocol ping, distinct from the ping tool
        "ping" => jsonrpc_result(&id, json!({})),
        "tools/list" => jsonrpc_result(&id, tools_list()),
        "tools/call" => match tools_call(&ctx, name_or_identity, auth, request.get("params")).await {
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

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "spacetimedb", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Tools for the addressed SpacetimeDB database: ping, get_schema, sql, and call. \
                         Use get_schema to see its tables and reducers, sql to query data, and call to \
                         invoke a reducer. Reducers are the usual way to write, and SQL writes require \
                         ownership. Everything runs with your identity, exactly as over the HTTP API.",
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "ping",
                "description": "Health check that echoes an optional message back.",
                "inputSchema": { "type": "object", "properties": { "message": { "type": "string" } } },
                "annotations": { "readOnlyHint": true }
            },
            {
                "name": "get_schema",
                "description": "Get the schema for this database as JSON, including its typespace, tables, and reducers.",
                "inputSchema": { "type": "object", "properties": {} },
                "annotations": { "readOnlyHint": true }
            },
            {
                "name": "sql",
                "description": "Run a SQL query against this database and return the rows as JSON. \
                                Write queries require ownership of the database.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sql": { "type": "string", "description": "The SQL statement to execute." },
                        "confirmed": { "type": "boolean", "description": "Wait for the read to be durably confirmed." }
                    },
                    "required": ["sql"]
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": true }
            },
            {
                "name": "call",
                "description": "Invoke a reducer with positional JSON arguments, for example [\"alice\"] or [42]. \
                                The reducer runs with your identity and is the standard way to write.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "reducer": { "type": "string", "description": "The name of the reducer to invoke." },
                        "args": { "type": "array", "description": "A JSON array of arguments to the reducer, in order. Omit or pass [] for none." }
                    },
                    "required": ["reducer"]
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": true }
            }
        ]
    })
}

async fn tools_call<S>(
    ctx: &S,
    name_or_identity: NameOrIdentity,
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
        "get_schema" => tool_get_schema(ctx, name_or_identity).await,
        "sql" => {
            let Some(sql) = arguments.and_then(|a| a.get("sql")).and_then(Value::as_str) else {
                return Err((INVALID_PARAMS, "sql argument must be a string".to_owned()));
            };
            let confirmed = arguments.and_then(|a| a.get("confirmed")).and_then(Value::as_bool);
            tool_sql(ctx, name_or_identity, auth, sql.to_owned(), confirmed).await
        }
        "call" => {
            let Some(reducer) = arguments.and_then(|a| a.get("reducer")).and_then(Value::as_str) else {
                return Err((INVALID_PARAMS, "reducer argument must be a string".to_owned()));
            };
            let args_json = reducer_args_json(arguments)?;
            tool_call_reducer(ctx, name_or_identity, auth, reducer.to_owned(), args_json).await
        }
        other => return Err((INVALID_PARAMS, format!("unknown tool: {other}"))),
    };

    Ok(match outcome {
        Ok(text) => json!({ "content": [ { "type": "text", "text": text } ], "isError": false }),
        Err(err) => execution_error_to_tool_result(err).await,
    })
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
        let info = initialize_result();
        assert_eq!(info["serverInfo"]["name"], "spacetimedb");
        assert_eq!(info["protocolVersion"], PROTOCOL_VERSION);
        assert!(info["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_exposes_the_expected_tools() {
        let listed = tools_list();
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
}
