use anyhow::{bail, ensure, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, Method, StatusCode, Url,
};
use serde::de::DeserializeOwned;
use spacetimedb_lib::{
    container::{endpoints::ContainerEndpoints, operations::*},
    Identity, Uuid,
};
use std::time::Duration;

const MAX_RESPONSE: usize = 64 * 1024;
pub(super) fn commands(command: Command) -> Command {
    let base = |name: &'static str, about: &'static str| {
        Command::new(name)
            .about(about)
            .arg(Arg::new("database").required(true).help("Database name or Identity"))
            .arg(crate::common_args::server())
            .arg(crate::common_args::yes())
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Print the typed response as JSON"),
            )
    };
    let action = |name, about| {
        base(name, about)
        .arg(Arg::new("request_id").long("request-id")
            .help("Retry an original UUIDv7 request; DATABASE must be its recorded Identity"))
        .after_help("Acceptance records the desired action; physical stop and readiness are asynchronous. A timeout must be retried with the original Identity and request ID printed on stderr.")
    };
    command
        .subcommand(base(
            "status",
            "Inspect container control state without opening its database",
        ))
        .subcommand(action("start", "Request container execution"))
        .subcommand(action("stop", "Request container stop"))
        .subcommand(action(
            "restart",
            "Request a new container instance and environment snapshot",
        ))
}

pub(super) async fn exec(config: &mut crate::Config, name: &str, args: &ArgMatches) -> Result<()> {
    let selection = args.get_one::<String>("server").map(String::as_str);
    let database = args.get_one::<String>("database").context("database is required")?;
    validate_database(database)?;
    let server = crate::container::publish::client::endpoint(&config.get_host_url(selection)?)?;
    let supplied_request = args
        .try_get_one::<String>("request_id")
        .ok()
        .flatten()
        .map(|id| Uuid::parse_str(id).context("request-id must be a UUIDv7"))
        .transpose()?;
    if supplied_request.is_some() {
        ensure!(
            database.parse::<Identity>().is_ok(),
            "retry with the original database Identity printed by the first command"
        );
    }
    let auth = crate::util::get_auth_header(config, false, selection, !args.get_flag("force")).await?;
    let client = ContainerClient::new(
        server,
        auth.to_header().context("container operations require a login")?,
    )?;
    if name == "status" {
        let status = client.status(database).await?;
        if args.get_flag("json") {
            println!("{}", serde_json::to_string(&status)?);
        } else {
            print_status(&status);
        }
        return Ok(());
    }
    let action = match name {
        "start" => ContainerAction::Start,
        "stop" => ContainerAction::Stop,
        "restart" => ContainerAction::Restart,
        _ => bail!("unsupported container operation"),
    };
    // Names are resolved once before mutation. The replay target is immutable.
    let identity = match database.parse::<Identity>() {
        Ok(identity) => identity,
        Err(_) => client.status(database).await?.database_identity,
    };
    let request_id = supplied_request.unwrap_or_else(|| Uuid::from_u128(uuid::Uuid::now_v7().as_u128()));
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?;
    spacetimedb_lib::deployment::operation_expiry_ms(request_id, now)
        .context("request ID is not within its seven-day retry window")?;
    // Structured retry parameters preserve the selected origin without
    // presenting unescaped server text as an executable shell command.
    let retry = serde_json::json!({
        "command": format!("container {}", action.path()),
        "database": identity.to_hex().to_string(),
        "request_id": request_id.to_string(),
        "server": client.server.as_str(),
    });
    eprintln!("Container request retry parameters: {retry}");
    let receipt = client.operate(identity, request_id, action).await.with_context(|| {
        format!("request did not return a verified receipt; retain these exact retry parameters: {retry}")
    })?;
    if args.get_flag("json") {
        println!("{}", serde_json::to_string(&receipt)?);
    } else {
        println!(
            "Accepted {} for {} at generation {}",
            action.path(),
            identity.to_hex(),
            receipt.generation
        );
    }
    Ok(())
}

pub(super) struct ContainerClient {
    pub(super) http: Client,
    server: Url,
    pub(super) authorization: HeaderValue,
}
impl ContainerClient {
    pub(super) fn new(server: Url, mut authorization: HeaderValue) -> Result<Self> {
        ensure!(
            authorization.as_bytes().starts_with(b"Bearer "),
            "container operations require ordinary Bearer authentication"
        );
        authorization.set_sensitive(true);
        let http = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            server,
            authorization,
        })
    }
    pub(super) fn url(&self, database: &str, operation: &str) -> Result<Url> {
        validate_database(database)?;
        let mut url = self.server.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid server origin"))?
            .pop_if_empty()
            .extend(["v1", "database", database, "container", operation]);
        Ok(url)
    }
    pub(super) async fn status(&self, database: &str) -> Result<ContainerStatus> {
        let response = self
            .http
            .get(self.url(database, "status")?)
            .header(AUTHORIZATION, self.authorization.clone())
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("container status could not be reached on the selected server"))?;
        let status: ContainerStatus = read_response(response, StatusCode::OK).await?;
        if let Ok(identity) = database.parse::<Identity>() {
            ensure!(
                status.database_identity == identity,
                "status returned another database Identity"
            );
        }
        validate_status(&status)?;
        Ok(status)
    }
    async fn operate(
        &self,
        identity: Identity,
        request_id: Uuid,
        action: ContainerAction,
    ) -> Result<ContainerOperationReceipt> {
        let response = self
            .http
            .request(Method::POST, self.url(identity.to_hex().as_ref(), action.path())?)
            .header(AUTHORIZATION, self.authorization.clone())
            .json(&ContainerOperationRequest { request_id })
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("container operation response was not received"))?;
        let receipt: ContainerOperationReceipt = read_response(response, StatusCode::ACCEPTED).await?;
        ensure!(
            receipt.database_identity == identity && receipt.request_id == request_id && receipt.action == action,
            "container operation receipt does not match this request"
        );
        Ok(receipt)
    }
}
fn validate_database(database: &str) -> Result<()> {
    ensure!(
        !database.is_empty()
            && database.len() <= 1024
            && !matches!(database, "." | "..")
            && !database.chars().any(char::is_control),
        "invalid database name or Identity"
    );
    Ok(())
}
async fn read_response<T: DeserializeOwned>(mut response: reqwest::Response, expected: StatusCode) -> Result<T> {
    let status = response.status();
    ensure!(
        response.content_length().is_none_or(|size| size <= MAX_RESPONSE as u64),
        "container response exceeds its size limit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("container response was interrupted"))?
    {
        ensure!(
            chunk.len() <= MAX_RESPONSE.saturating_sub(bytes.len()),
            "container response exceeds its size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    if status != expected {
        let error = serde_json::from_slice::<ContainerApiError>(&bytes)
            .ok()
            .map(|value| value.error);
        bail!(
            "{} (HTTP {})",
            match error {
                Some(ContainerErrorCode::AccessDenied) => "database role does not permit this operation",
                Some(ContainerErrorCode::NotFound) => "database was not found",
                Some(ContainerErrorCode::InvalidRequest) => "container request is invalid or expired",
                Some(ContainerErrorCode::Conflict) => "container request conflicts with current state",
                Some(ContainerErrorCode::OutcomeUnknown) =>
                    "container request outcome is unknown; retry the same request ID",
                _ => "container service is unavailable on the selected server",
            },
            status.as_u16()
        );
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid container response"))
}
fn validate_status(status: &ContainerStatus) -> Result<()> {
    if let Some(state) = &status.operational
        && let Some(instance) = &state.current_instance
    {
        ensure!(
            instance.generation == state.generation,
            "status instance is not the current generation"
        );
        if let Some(id) = &instance.applied_env_generation {
            ensure!(Uuid::parse_str(id).is_ok(), "invalid applied environment generation");
        }
    }
    if let EndpointStatus::Available { endpoints } = &status.endpoints {
        super::url::validate(&ContainerEndpoints {
            database_identity: status.database_identity,
            endpoints: endpoints.clone(),
        })?;
    }
    Ok(())
}
fn print_status(status: &ContainerStatus) {
    println!("Database: {}", status.database_identity.to_hex());
    if !status.published {
        println!("Container: none published");
    }
    if let Some(configuration) = &status.configuration {
        println!("Published image: {}", configuration.image_digest);
        let limits = &configuration.resources;
        println!(
            "Published limits: {} millicores, {} memory bytes, {} scratch bytes, {} tasks",
            limits.cpu_millicores, limits.memory_bytes, limits.scratch_bytes, limits.pids_max
        );
    }
    if let Some(state) = &status.operational {
        println!("Desired: {:?} (generation {})", state.desired_state, state.generation);
        println!("Condition: {:?}", state.condition);
        if let Some(instance) = &state.current_instance {
            println!("Observed: {:?}", instance.state);
            if let Some(usage) = &instance.usage {
                println!(
                    "Reported usage: sample {}{}",
                    usage.sample_sequence,
                    if usage.final_report { " (final totals)" } else { "" }
                );
                let totals = &usage.cumulative;
                println!(
                    "CPU: {} ns; memory: {} byte-seconds; scratch: {} byte-seconds; sent: {} bytes; received: {} bytes",
                    totals.cpu_nanoseconds,
                    totals.memory_byte_seconds,
                    totals.scratch_byte_seconds,
                    totals.transmitted_bytes,
                    totals.received_bytes
                );
                if usage.measurement_interrupted {
                    println!("Measurement interrupted by host restart; totals include known usage only.");
                }
            } else {
                println!("Usage: not reported for this generation");
            }
            if let Some(environment) = &instance.applied_env_generation {
                println!("Environment generation: {environment}");
            }
            if let Some(code) = instance.exit_code {
                println!("Exit code: {code}");
            }
            if instance.oom_killed {
                println!("Out of memory: yes");
            }
        } else {
            println!("Observed: no instance admitted for this generation");
        }
    }
    match &status.endpoints {
        EndpointStatus::Available { endpoints } => {
            for endpoint in endpoints {
                println!("{}: {}", endpoint.name, endpoint.url);
            }
        }
        EndpointStatus::Pending => println!("Endpoints: pending"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_path_encodes_one_database_and_rejects_dot_segments() {
        let client = ContainerClient::new(
            Url::parse("http://127.0.0.1:43123/").unwrap(),
            HeaderValue::from_static("Bearer unusable"),
        )
        .unwrap();
        assert_eq!(
            client.url("name/child?x#y", "status").unwrap().path(),
            "/v1/database/name%2Fchild%3Fx%23y/container/status"
        );
        for invalid in ["", ".", "..", "name\n"] {
            assert!(client.url(invalid, "status").is_err());
        }
    }
}
