//! Read-only conveniences over the private st_env SQL table.
use anyhow::{ensure, Context};
use clap::{Arg, ArgAction, ArgMatches, Command};
use spacetimedb_lib::environment::{validate_key, MAX_ENV_KEY_BYTES, MAX_ENV_VALUE_BYTES, MAX_ENV_VARS};

use super::{
    db_arg_resolution::{load_config_db_targets, resolve_database_arg},
    sql,
};
use crate::{api::ClientApi, common_args, Config};

pub fn cli() -> Command {
    let target = |command: Command| {
        command
            .arg(
                Arg::new("database")
                    .index(1)
                    .required(true)
                    .help("The database name, identity, or configured target"),
            )
            .arg(common_args::server())
            .arg(common_args::anonymous())
            .arg(common_args::yes())
            .arg(common_args::confirmed())
            .arg(
                Arg::new("no_config")
                    .long("no-config")
                    .action(ArgAction::SetTrue)
                    .help("Ignore project configuration when resolving the database target"),
            )
    };
    Command::new("env")
        .about("Inspect published database environment variables")
        .subcommand_required(true)
        .subcommand(target(
            Command::new("get").about("Read one published environment value").arg(
                Arg::new("key")
                    .index(2)
                    .required(true)
                    .help("The declared environment key to read"),
            ),
        ))
        .subcommand(target(
            Command::new("list").about("List published environment keys (never values)"),
        ))
}

#[derive(Clone)]
enum Query {
    List,
    Get(String),
}
impl Query {
    fn sql(&self) -> anyhow::Result<String> {
        match self {
            Self::List => Ok("SELECT key FROM st_env".into()),
            Self::Get(key) => {
                // POSIX names cannot contain quotes or SQL syntax.
                validate_key(key).map_err(|_| anyhow::anyhow!("Invalid environment key name"))?;
                Ok(format!("SELECT value FROM st_env WHERE key = '{key}'"))
            }
        }
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let (command, args) = args.subcommand().context("Expected env get or list")?;
    let query = match command {
        "list" => Query::List,
        "get" => Query::Get(
            args.get_one::<String>("key")
                .context("Expected environment key")?
                .clone(),
        ),
        _ => anyhow::bail!("Environment values can only be changed by publishing"),
    };
    query.sql()?;
    let targets = load_config_db_targets(args.get_flag("no_config"))?;
    let database = resolve_database_arg(
        args.get_one::<String>("database").map(String::as_str),
        targets.as_deref(),
        "spacetime env get/list <database>",
    )?;
    let con = sql::parse_req(config, args, &database.database, database.server.as_deref()).await?;
    let mut request = ClientApi::new(con).sql();
    if let Some(confirmed) = args.get_one::<bool>("confirmed") {
        request = request.query(&[("confirmed", confirmed)]);
    }
    print!("{}", fetch(request, query).await?);
    Ok(())
}

async fn fetch(request: reqwest::RequestBuilder, query: Query) -> anyhow::Result<String> {
    use futures::StreamExt;
    let response = request
        .timeout(std::time::Duration::from_secs(30))
        .body(query.sql()?)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "Environment read failed with HTTP {}",
        response.status()
    );
    let mut body = Vec::new();
    let limit = MAX_ENV_VARS * (MAX_ENV_KEY_BYTES + MAX_ENV_VALUE_BYTES) * 6 + 64 * 1024;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            body.len().saturating_add(chunk.len()) <= limit,
            "Environment read response exceeds limit"
        );
        body.extend_from_slice(&chunk);
    }
    render(&body, &query)
}

fn render(body: &[u8], query: &Query) -> anyhow::Result<String> {
    // Only project the requested single string column; do not dump an error or
    // unexpected response which could contain unrequested secret values.
    let results: Vec<spacetimedb_client_api_messages::http::SqlStmtResult<Vec<String>>> =
        serde_json::from_slice(body).map_err(|_| anyhow::anyhow!("Invalid environment read response"))?;
    ensure!(results.len() == 1, "Invalid environment read result count");
    let result = &results[0];
    let expected = match query {
        Query::List => "key",
        Query::Get(_) => "value",
    };
    ensure!(
        result.schema.elements.len() == 1
            && result.schema.elements[0].name.as_deref() == Some(expected)
            && result.schema.elements[0].algebraic_type == spacetimedb_lib::AlgebraicType::String,
        "Invalid environment read projection"
    );
    let mut values = Vec::new();
    for row in &result.rows {
        ensure!(row.len() == 1, "Invalid environment read row");
        if matches!(query, Query::List) {
            validate_key(&row[0]).context("Invalid environment key in response")?;
        }
        ensure!(
            row[0].len() <= MAX_ENV_VALUE_BYTES,
            "Environment read value exceeds limit"
        );
        values.push(row[0].as_str());
    }
    match query {
        Query::List => {
            ensure!(values.len() <= MAX_ENV_VARS, "Environment key count exceeds limit");
            values.sort_unstable();
        }
        Query::Get(_) => ensure!(values.len() == 1, "Environment key is absent"),
    }
    Ok(values.into_iter().map(|v| format!("{v}\n")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(column: &'static str, rows: Vec<Vec<&str>>) -> Vec<u8> {
        serde_json::to_vec(&[spacetimedb_client_api_messages::http::SqlStmtResult {
            schema: spacetimedb_lib::sats::ProductType::from([(column, spacetimedb_lib::AlgebraicType::String)]),
            rows,
            total_duration_micros: 0,
            stats: Default::default(),
        }])
        .unwrap()
    }
    #[test]
    fn read_only_commands_and_safe_queries() {
        for command in ["set", "del", "delete", "update"] {
            assert!(cli().try_get_matches_from(["env", command, "db", "KEY"]).is_err());
        }
        let matches = cli()
            .try_get_matches_from([
                "env",
                "get",
                "db",
                "KEY",
                "--server",
                "http://127.0.0.1:9",
                "--no-config",
            ])
            .unwrap();
        let get = matches.subcommand_matches("get").unwrap();
        assert_eq!(get.get_one::<String>("database").unwrap(), "db");
        assert_eq!(get.get_one::<String>("key").unwrap(), "KEY");
        assert_eq!(Query::List.sql().unwrap(), "SELECT key FROM st_env");
        assert_eq!(
            Query::Get("KEY".into()).sql().unwrap(),
            "SELECT value FROM st_env WHERE key = 'KEY'"
        );
        assert!(Query::Get("x';DELETE FROM st_env;--".into()).sql().is_err());
    }
    #[test]
    fn list_projects_keys_and_rejects_unexpected_secret_columns() {
        assert_eq!(
            render(&body("key", vec![vec!["Z"], vec!["A"]]), &Query::List).unwrap(),
            "A\nZ\n"
        );
        let err = render(&body("value", vec![vec!["generated-secret-sentinel"]]), &Query::List).unwrap_err();
        assert!(!format!("{err:#}").contains("generated-secret-sentinel"));
        assert_eq!(
            render(&body("value", vec![vec![""]]), &Query::Get("A".into())).unwrap(),
            "\n"
        );
        assert!(render(&body("value", vec![]), &Query::Get("A".into())).is_err());
    }
    #[tokio::test]
    async fn actual_loopback_queries_and_error_redaction() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (query, status, response, expected) in [
            (Query::List, "200 OK", body("key", vec![vec!["KEY"]]), Some("KEY\n")),
            (
                Query::Get("KEY".into()),
                "200 OK",
                body("value", vec![vec!["generated-read-sentinel"]]),
                Some("generated-read-sentinel\n"),
            ),
            (
                Query::Get("KEY".into()),
                "403 Forbidden",
                b"generated-error-secret".to_vec(),
                None,
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let expected_sql = query.sql().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut input = Vec::new();
                let (head, length) = loop {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    input.extend_from_slice(&chunk[..n]);
                    assert!(input.len() <= 16384);
                    if let Some(end) = input.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&input[..end]).unwrap();
                        assert!(headers.starts_with("POST /v1/database/owned/sql HTTP/1.1\r\n"));
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .map(str::to_owned)
                            })
                            .unwrap()
                            .parse()
                            .unwrap();
                        break (end + 4, length);
                    }
                };
                while input.len() < head + length {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    input.extend_from_slice(&chunk[..n]);
                }
                assert_eq!(&input[head..head + length], expected_sql.as_bytes());
                let headers = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len());
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&response).await.unwrap();
                stream.shutdown().await.unwrap();
            });
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            let result = fetch(client.post(format!("http://{address}/v1/database/owned/sql")), query).await;
            match expected {
                Some(expected) => assert_eq!(result.unwrap(), expected),
                None => assert!(!format!("{:#}", result.unwrap_err()).contains("generated-error-secret")),
            }
            tokio::time::timeout(std::time::Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
