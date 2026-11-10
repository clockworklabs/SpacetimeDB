//! Stateful checks against explicit expected results, independent of the golden implementation.
use super::scorers::call_reducer_json_out;
use super::{casing_for_lang, derive_cat_task_from_file, ident, table_name, Lang, ScoreDetails, Scorer};
use crate::bench::utils::sanitize_db_name;
use anyhow::{anyhow, bail, ensure, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Scenario {
    pub server: String,
    pub db: String,
    pub lang: Lang,
    check: fn(&Scenario) -> Result<()>,
}

pub fn scenario(
    source: &str,
    route: &str,
    server: &str,
    lang: Lang,
    check: fn(&Scenario) -> Result<()>,
) -> Box<dyn Scorer> {
    let (category, task) = derive_cat_task_from_file(source);
    Box::new(Scenario {
        server: server.into(),
        db: sanitize_db_name(&format!("{category}-{task}-{route}-llm")),
        lang,
        check,
    })
}

impl Scorer for Scenario {
    fn id(&self) -> &'static str {
        "behavioral_scenario"
    }

    fn score(&self, _: &str) -> ScoreDetails {
        // Scoring can run inside Tokio. Keep blocking HTTP clients outside its runtime.
        let result = std::thread::scope(|scope| scope.spawn(|| (self.check)(self)).join());
        let result = result.unwrap_or_else(|_| Err(anyhow!("scenario panicked")));
        ScoreDetails {
            pass: result.is_ok(),
            partial: if result.is_ok() { 1.0 } else { 0.0 },
            notes: match result {
                Ok(()) => json!({"checks": "all scenario assertions passed"}),
                Err(error) => json!({"error": format!("{error:#}")}),
            },
        }
    }
}

impl Scenario {
    pub fn connect(&self, token: &str) -> Result<Connection> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let address = self
            .server
            .strip_prefix("http://")
            .ok_or_else(|| anyhow!("local HTTP server required"))?;
        let stream = std::net::TcpStream::connect_timeout(&address.parse()?, Duration::from_secs(10))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let mut request =
            format!("ws://{address}/v1/database/{}/subscribe?compression=None", self.db).into_client_request()?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "v1.json.spacetimedb".parse()?);
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse()?);
        let (socket, _) = tokio_tungstenite::tungstenite::client(request, stream)?;
        let mut connection = Connection { socket };
        let hello = connection.next_message()?;
        ensure!(
            hello.get("IdentityToken").is_some(),
            "expected identity handshake, got {hello}"
        );
        Ok(connection)
    }

    pub fn new_user(&self) -> Result<(String, Value)> {
        let value: Value = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?
            .post(format!("{}/v1/identity", self.server))
            .send()?
            .error_for_status()?
            .json()?;
        let token = value["token"]
            .as_str()
            .ok_or_else(|| anyhow!("missing identity token"))?
            .to_owned();
        ensure!(!value["identity"].is_null(), "missing identity");
        let identity = spacetimedb_lib::Identity::from_hex(
            value["identity"]
                .as_str()
                .ok_or_else(|| anyhow!("identity is not hex"))?,
        )?;
        Ok((
            token,
            serde_json::to_value(spacetimedb_lib::ser::serde::SerializeWrapper::from_ref(&identity))?,
        ))
    }

    pub fn call_as(&self, token: &str, reducer: &str, args: Value) -> Result<(u16, String)> {
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?
            .post(format!("{}/v1/database/{}/call/{reducer}", self.server, self.db))
            .bearer_auth(token)
            .json(&args)
            .send()?;
        Ok((response.status().as_u16(), response.text()?))
    }

    pub fn expect_call_as(&self, token: &str, reducer: &str, args: Value, error: Option<&str>) -> Result<()> {
        let (status, body) = self.call_as(token, reducer, args)?;
        match error {
            None => ensure!((200..300).contains(&status), "{reducer}: HTTP {status}: {body}"),
            Some(expected) => ensure!(
                status == 530 && body.contains(expected),
                "{reducer}: expected reducer error {expected:?}, got HTTP {status}: {body}"
            ),
        }
        Ok(())
    }

    pub fn reject_scheduled(&self, reducer: &str, args: Value) -> Result<()> {
        let (token, _) = self.new_user()?;
        let (status, body) = self.call_as(&token, reducer, args)?;
        // Current SpacetimeDB makes scheduled callbacks private. Older servers reach the identity guard.
        ensure!(
            (status == 404 && (body.contains("No such reducer") || body.contains("No such procedure")))
                || (status == 530 && body.contains("scheduler only")),
            "external scheduled call {reducer}: HTTP {status}: {body}"
        );
        Ok(())
    }

    pub fn sql_as(&self, token: &str, query: &str) -> Result<(u16, String)> {
        let mut request = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?
            .post(format!("{}/v1/database/{}/sql", self.server, self.db));
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
        let response = request.body(query.to_owned()).send()?;
        Ok((response.status().as_u16(), response.text()?))
    }

    pub fn rows_as(&self, token: &str, table: &str, columns: &[&str], expected: Value) -> Result<()> {
        let query = self.query(table, columns);
        let (status, body) = self.sql_as(token, &query)?;
        ensure!(status == 200, "{query}: HTTP {status}: {body}");
        let result: Value = serde_json::from_str(&body)?;
        compare_rows(&result[0]["rows"], &expected).map_err(|e| anyhow!("{query}: {e}"))
    }

    pub fn http(&self, path: &str, body: &str) -> Result<(u16, String)> {
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?
            .post(format!("{}/v1/database/{}/route{path}", self.server, self.db))
            .header("content-type", "text/plain")
            .body(body.to_owned())
            .send()?;
        Ok((response.status().as_u16(), response.text()?))
    }

    pub fn expect_http(&self, path: &str, body: &str, status: u16, expected: &str) -> Result<()> {
        let actual = self.http(path, body)?;
        ensure!(
            actual == (status, expected.into()),
            "POST {path} {body:?}: expected {status} {expected:?}, got {actual:?}"
        );
        Ok(())
    }

    pub fn call(&self, reducer: &str, args: Value) -> Result<()> {
        self.call_result(reducer, &args)
            .map(|_| ())
            .map_err(|e| anyhow!("{reducer}({args}): {e}"))
    }

    pub fn output(&self, procedure: &str, args: Value, expected: Value) -> Result<()> {
        let output = self
            .call_result(procedure, &args)
            .map_err(|e| anyhow!("{procedure}: {e}"))?;
        let actual: Value = serde_json::from_str(&output)?;
        ensure!(actual == expected, "{procedure}: expected {expected}, got {actual}");
        Ok(())
    }

    fn call_result(&self, reducer: &str, args: &Value) -> Result<String, String> {
        let args = args.as_array().ok_or("reducer arguments must be an array")?;
        call_reducer_json_out(&self.db, reducer, args, Some(&self.server))
    }

    /// Require the contract's error text. Transport errors and missing reducers must not pass.
    pub fn reject(&self, reducer: &str, args: Value, expected: &str) -> Result<()> {
        match self.call_result(reducer, &args) {
            Err(error) if error.contains("(530") && error.contains(expected) => Ok(()),
            actual => bail!("{reducer}({args}): expected error {expected:?}, got {actual:?}"),
        }
    }

    pub fn query(&self, table: &str, columns: &[&str]) -> String {
        let columns = columns
            .iter()
            .map(|c| format!("\"{}\"", ident(c, casing_for_lang(self.lang))))
            .collect::<Vec<_>>()
            .join(", ");
        format!("SELECT {columns} FROM \"{}\"", table_name(table, self.lang))
    }

    pub fn rows(&self, table: &str, columns: &[&str], expected: Value) -> Result<()> {
        self.rows_as("", table, columns, expected)
    }

    /// HTTP SQL runs connection lifecycle hooks too. Exclude the observer's own identity.
    pub fn presence_rows(&self, table: &str, owner: &str, columns: &[&str], expected: Value) -> Result<()> {
        let (token, identity) = self.new_user()?;
        let spacetimedb_lib::de::serde::DeserializeWrapper(identity): spacetimedb_lib::de::serde::DeserializeWrapper<
            spacetimedb_lib::Identity,
        > = serde_json::from_value(identity)?;
        let query = format!(
            "{} WHERE \"{}\" != 0x{}",
            self.query(table, columns),
            ident(owner, casing_for_lang(self.lang)),
            identity.to_hex()
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (status, body) = self.sql_as(&token, &query)?;
            ensure!(status == 200, "{query}: HTTP {status}: {body}");
            let actual: Value = serde_json::from_str(&body)?;
            match compare_rows(&actual[0]["rows"], &expected) {
                Ok(()) => return Ok(()),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    pub fn eventually_rows(&self, table: &str, columns: &[&str], expected: Value) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match self.rows(table, columns, expected.clone()) {
                Ok(()) => return Ok(()),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    pub fn eventually_count(&self, table: &str, expected: i64) -> Result<()> {
        let query = format!("SELECT COUNT(*) AS n FROM \"{}\"", table_name(table, self.lang));
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (status, body) = self.sql_as("", &query)?;
            ensure!(status == 200, "{query}: HTTP {status}: {body}");
            let result: Value = serde_json::from_str(&body)?;
            let count = result[0]["rows"][0][0]
                .as_i64()
                .ok_or_else(|| anyhow!("invalid count: {result}"))?;
            if count == expected {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "{query}: expected {expected}, got {count}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn compare_rows(actual: &Value, expected: &Value) -> Result<()> {
    let sorted = |v: &Value| -> Result<Vec<String>> {
        let mut rows = v
            .as_array()
            .ok_or_else(|| anyhow!("expected array, got {v}"))?
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>();
        rows.sort();
        Ok(rows)
    };
    ensure!(
        sorted(actual)? == sorted(expected)?,
        "expected {expected}, got {actual}"
    );
    Ok(())
}

pub struct Connection {
    socket: tokio_tungstenite::tungstenite::WebSocket<std::net::TcpStream>,
}

impl Connection {
    pub fn next_message(&mut self) -> Result<Value> {
        use tokio_tungstenite::tungstenite::Message;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| anyhow!("websocket message timeout"))?;
            self.socket.get_mut().set_read_timeout(Some(remaining))?;
            match self.socket.read()? {
                Message::Text(text) => return Ok(serde_json::from_str(&text)?),
                Message::Ping(_) => self.socket.flush()?,
                Message::Pong(_) => (),
                other => bail!("unexpected websocket frame: {other:?}"),
            }
        }
    }

    pub fn subscribe(&mut self, id: u32, query: &str) -> Result<()> {
        self.socket.send(tokio_tungstenite::tungstenite::Message::Text(
            json!({"SubscribeSingle": {
                "query": query, "request_id": id, "query_id": {"id": id}
            }})
            .to_string()
            .into(),
        ))?;
        Ok(())
    }

    pub fn expect_update(&mut self, kind: &str, inserts: Value, deletes: Value) -> Result<()> {
        let message = self.next_message()?;
        ensure!(
            message.get(kind).is_some()
                || (kind == "TransactionUpdate" && message.get("TransactionUpdateLight").is_some()),
            "expected {kind}, got {message}"
        );
        compare_rows(&Value::Array(update_rows(&message, "inserts")?), &inserts)?;
        compare_rows(&Value::Array(update_rows(&message, "deletes")?), &deletes)?;
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.socket.close(None);
        let _ = self.socket.get_mut().shutdown(std::net::Shutdown::Both);
    }
}

fn update_rows(value: &Value, key: &str) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    match value {
        Value::Object(object) => {
            for (name, value) in object {
                if name == key {
                    for row in value
                        .as_array()
                        .ok_or_else(|| anyhow!("invalid update rows: {value}"))?
                    {
                        rows.push(match row.as_str() {
                            Some(text) => serde_json::from_str(text)?,
                            None => row.clone(),
                        });
                    }
                } else {
                    rows.extend(update_rows(value, key)?);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                rows.extend(update_rows(value, key)?);
            }
        }
        _ => (),
    }
    Ok(rows)
}

/// Local, deterministic upstream for the procedure-cache benchmark. No external service or API key.
pub struct HttpFixture {
    pub url: String,
    requests: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    fail: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<()>>>,
}

impl HttpFixture {
    pub fn start() -> Result<Self> {
        use std::io::{Read, Write};
        use std::sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}/value", listener.local_addr()?);
        let requests = Arc::new(AtomicUsize::new(0));
        let fail = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, failing, stopping) = (requests.clone(), fail.clone(), stop.clone());
        let worker = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                // Accepted sockets inherit the listener's nonblocking mode on Windows.
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    ensure!(request.len() < 8192, "upstream request headers too large");
                    let mut byte = [0];
                    stream.read_exact(&mut byte)?;
                    request.push(byte[0]);
                }
                let n = count.fetch_add(1, Ordering::SeqCst) + 1;
                let (status, body) = if failing.load(Ordering::SeqCst) {
                    ("503 Service Unavailable", "unavailable".into())
                } else {
                    ("200 OK", format!("value-{n}"))
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
            }
            Ok(())
        });
        Ok(Self {
            url,
            requests,
            fail,
            stop,
            worker: Some(worker),
        })
    }

    pub fn requests(&self) -> usize {
        self.requests.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn fail(&self, fail: bool) {
        self.fail.store(fail, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn finish(mut self) -> Result<()> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow!("upstream worker panicked"))?
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_fixture_waits_for_fragmented_request_headers() {
        use std::io::{Read, Write};
        let fixture = HttpFixture::start().unwrap();
        let address = fixture
            .url
            .strip_prefix("http://")
            .unwrap()
            .strip_suffix("/value")
            .unwrap();
        let result = (|| -> Result<()> {
            for n in 1..=3 {
                let mut stream = std::net::TcpStream::connect(address)?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;
                stream.write_all(b"GET /value HTTP/1.1\r\n")?;
                std::thread::sleep(Duration::from_millis(30));
                stream.write_all(b"Host: localhost\r\n\r\n")?;
                let mut response = String::new();
                stream.read_to_string(&mut response)?;
                ensure!(
                    response.ends_with(&format!("\r\n\r\nvalue-{n}")),
                    "unexpected response: {response}"
                );
            }
            Ok(())
        })();
        fixture.finish().unwrap();
        result.unwrap();
    }

    #[test]
    fn subscription_rows_decode_wire_json_and_reject_malformed_rows() {
        let update = json!({"TransactionUpdateLight": {"tables": [
            {"updates": [{"inserts": ["[1,\"a\"]"], "deletes": ["[2,\"b\"]"]}]}
        ]}});
        assert_eq!(update_rows(&update, "inserts").unwrap(), vec![json!([1, "a"])]);
        assert_eq!(update_rows(&update, "deletes").unwrap(), vec![json!([2, "b"])]);
        assert!(update_rows(&json!({"inserts": ["invalid json"]}), "inserts").is_err());
        assert!(update_rows(&json!({"inserts": null}), "inserts").is_err());
    }

    #[test]
    fn row_checks_ignore_order_but_reject_wrong_values_duplicates_and_missing_rows() {
        let expected = json!([[1, "a"], [2, "b"]]);
        assert!(compare_rows(&json!([[2, "b"], [1, "a"]]), &expected).is_ok());
        for wrong in [
            json!([[1, "a"]]),
            json!([[1, "a"], [1, "a"], [2, "b"]]),
            json!([[1, "a"], [2, "c"]]),
            Value::Null,
        ] {
            assert!(compare_rows(&wrong, &expected).is_err());
        }
    }
}
