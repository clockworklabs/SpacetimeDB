use crate::util::decode_claims;
use crate::version::CLI_VERSION;
use crate::Config;
use clap::{ArgMatches, Command};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

const DISABLED_ENV: &str = "SPACETIME_TELEMETRY_DISABLED";
const DEBUG_ENV: &str = "SPACETIME_TELEMETRY_DEBUG";
const AMPLITUDE_URL: &str = "https://api2.amplitude.com/2/httpapi";
const SPACETIMEDB_COM_ISSUER: &str = "https://auth.spacetimedb.com";
const MAINCLOUD_HOST: &str = "maincloud.spacetimedb.com";

const WHAT_IS_SENT: &str = "When you are logged in with spacetimedb.com, `spacetime publish`, `login`, `call`, `logs` and `sql` report the command, the CLI version, your OS and CPU architecture, linked to your account. `publish`, `call`, `logs` and `sql` only report when they target Maincloud.";
const OPT_OUT: &str = "To opt out, run `spacetime telemetry disable` or set SPACETIME_TELEMETRY_DISABLED=1.";

// the event of the running command, waited for by `flush`
static PENDING: Mutex<Option<oneshot::Receiver<()>>> = Mutex::new(None);

pub fn cli() -> Command {
    Command::new("telemetry")
        .subcommands([
            Command::new("enable").about("Send usage telemetry"),
            Command::new("disable").about("Stop sending usage telemetry"),
        ])
        .about("Show, enable or disable the usage telemetry the CLI sends")
        .long_about(format!(
            "Show, enable or disable the usage telemetry the CLI sends.\n\n{WHAT_IS_SENT}\n\n{OPT_OUT} \
             Set {DEBUG_ENV}=1 to print the events instead of sending them."
        ))
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    if let Some((cmd, _)) = args.subcommand() {
        let enabled = match cmd {
            "enable" => true,
            "disable" => false,
            unknown => return Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
        };
        config.set_telemetry(enabled);
        config.save();
    }

    if is_set(DISABLED_ENV) {
        println!("Status: Disabled ({DISABLED_ENV} is set)");
    } else if config.telemetry() == Some(false) {
        println!("Status: Disabled");
    } else if amplitude_api_key().is_none() {
        println!("Status: Enabled, but this build of the CLI does not send telemetry");
    } else {
        println!("Status: Enabled");
        println!("{OPT_OUT}");
    }

    Ok(())
}

/// Reports `spacetime <command>` when telemetry is on and the user is logged in with spacetimedb.com.
/// Await the returned receiver before exiting, or the event can be lost.
pub fn track(config: &mut Config, command: &str) -> Option<oneshot::Receiver<()>> {
    let debug = is_set(DEBUG_ENV);
    let api_key = amplitude_api_key();
    if is_set(DISABLED_ENV) || config.telemetry() == Some(false) || (api_key.is_none() && !debug) {
        return None;
    }

    let user_id = user_id(config.spacetimedb_token()?)?;

    if config.telemetry().is_none() {
        // only show the notice once it is saved, or it would repeat on every run
        if !config.can_save() {
            return None;
        }
        config.set_telemetry(true);
        if !config.save() {
            return None;
        }
        eprintln!("Attention: the SpacetimeDB CLI now collects usage telemetry.\n{WHAT_IS_SENT}\n{OPT_OUT}\n");
    }

    let event = event(command, &user_id);
    match api_key {
        Some(api_key) if !debug => Some(send_in_background(api_key, event)),
        _ => {
            eprintln!("[telemetry] {event:#}");
            None
        }
    }
}

/// Like `track`, for the commands that only report when `server` is Maincloud. Returns whether it is.
/// `flush` waits for the event.
pub fn track_maincloud(config: &mut Config, command: &str, server: Option<&str>) -> bool {
    let maincloud = is_maincloud(config, server);
    if maincloud {
        *PENDING.lock().unwrap() = track(config, command);
    }
    maincloud
}

/// Waits for the event `track_maincloud` started, if any.
pub async fn flush() {
    let event = PENDING.lock().unwrap().take();
    if let Some(event) = event {
        let _ = event.await;
    }
}

fn is_maincloud(config: &Config, server: Option<&str>) -> bool {
    config.host(server).is_ok_and(|host| host == MAINCLOUD_HOST)
}

fn amplitude_api_key() -> Option<&'static str> {
    option_env!("SPACETIME_AMPLITUDE_API_KEY").filter(|key| !key.is_empty())
}

fn is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn user_id(token: &str) -> Option<String> {
    let claims = decode_claims(token).ok()?;
    (&*claims.issuer == SPACETIMEDB_COM_ISSUER).then(|| format!("spacetimedb_{}", claims.subject))
}

fn event(command: &str, user_id: &str) -> Value {
    json!({
        "event_type": format!("cli/{command}"),
        "user_id": user_id,
        "event_properties": {
            "cli_version": CLI_VERSION,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
    })
}

fn send_in_background(api_key: &'static str, event: Value) -> oneshot::Receiver<()> {
    let (sent, done) = oneshot::channel();
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            return;
        };
        runtime.block_on(send(AMPLITUDE_URL, api_key, event));
        let _ = sent.send(());
    });
    done
}

async fn send(url: &str, api_key: &str, event: Value) {
    // best effort
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(2)).build() else {
        return;
    };
    let _ = client
        .post(url)
        .json(&json!({ "api_key": api_key, "events": [event] }))
        .send()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
    use spacetimedb_paths::cli::CliTomlPath;
    use spacetimedb_paths::FromPathUnchecked;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn token(issuer: &str) -> String {
        let claims =
            json!({ "sub": "01JABCDEFGHJKMNPQRSTVWXYZ0", "iss": issuer, "aud": "spacetimedb", "iat": 1_700_000_000 });
        format!("e30.{}.c2ln", STANDARD_NO_PAD.encode(claims.to_string()))
    }

    #[test]
    fn user_id_only_for_spacetimedb_com_tokens() {
        assert_eq!(
            user_id(&token(SPACETIMEDB_COM_ISSUER)).as_deref(),
            Some("spacetimedb_01JABCDEFGHJKMNPQRSTVWXYZ0")
        );
        assert_eq!(user_id(&token("localhost")), None);
        assert_eq!(user_id(&token("https://auth.spacetimedb.com/oidc")), None);
        assert_eq!(user_id("not a token"), None);
    }

    #[test]
    fn only_the_maincloud_host_is_maincloud() {
        let config = Config::new_with_localhost(CliTomlPath::from_path_unchecked("cli.toml"));

        assert!(is_maincloud(&config, None));
        assert!(is_maincloud(&config, Some("maincloud")));
        assert!(is_maincloud(&config, Some("https://maincloud.spacetimedb.com")));
        assert!(!is_maincloud(&config, Some("local")));
        assert!(!is_maincloud(&config, Some("http://127.0.0.1:3000")));
        assert!(!is_maincloud(&config, Some("unknown")));
    }

    #[tokio::test]
    async fn send_posts_the_event_with_the_api_key() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/2/httpapi", listener.local_addr().unwrap());

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            // read until the json body is complete, a single read can return a partial request
            while !request.ends_with(b"}]}") {
                let n = socket.read(&mut buf).await.unwrap();
                assert_ne!(n, 0, "connection closed before the body arrived");
                request.extend_from_slice(&buf[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}")
                .await
                .unwrap();
            String::from_utf8(request).unwrap()
        });

        send(&url, "test-key", event("sql", "spacetimedb_user")).await;

        let request = server.await.unwrap();
        let (head, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("POST /2/httpapi HTTP/1.1"), "{head}");
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["api_key"], "test-key");
        assert_eq!(body["events"][0]["event_type"], "cli/sql");
        assert_eq!(body["events"][0]["user_id"], "spacetimedb_user");
        assert_eq!(body["events"][0]["event_properties"]["cli_version"], CLI_VERSION);
    }

    #[tokio::test]
    async fn send_gives_up_on_a_server_that_never_answers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/2/httpapi", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let _connection = listener.accept().await;
            std::future::pending::<()>().await;
        });

        let start = Instant::now();
        send(&url, "test-key", event("logs", "spacetimedb_user")).await;

        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(4),
            "{elapsed:?}"
        );
        server.abort();
    }
}
